use crate::{
    builtin::BUILTIN,
    elfloader_error,
    handle::{ELFHandle, ELFLibrary},
    parse_err_convert,
    segment::ELFSegments,
    Error, Phdr, Rela, Result, MASK, PAGE_SIZE, REL_BIT, REL_MASK,
};
use core::ptr::NonNull;
use elf::abi::*;
use snafu::ResultExt;

#[derive(Debug)]
pub(crate) struct ELFRelocation {
    pub(crate) pltrel: Option<&'static [Rela]>,
    pub(crate) rel: Option<&'static [Rela]>,
    pub(crate) relro: Option<ELFRelro>,
}

#[allow(unused)]
#[derive(Debug)]
pub(crate) struct ELFRelro {
    addr: usize,
    len: usize,
}

impl ELFRelro {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> ELFRelro {
        ELFRelro {
            addr: segments.base() + phdr.p_vaddr as usize,
            len: phdr.p_memsz as usize,
        }
    }

    #[inline]
    fn relro(&self) -> Result<()> {
        #[cfg(feature = "mmap")]
        {
            use crate::ErrnoSnafu;
            use nix::sys::mman;
            let end = (self.addr + self.len + PAGE_SIZE - 1) & MASK;
            let start = self.addr & MASK;
            let start_addr = unsafe { NonNull::new_unchecked(start as _) };
            unsafe {
                mman::mprotect(start_addr, end - start, mman::ProtFlags::PROT_READ)
                    .context(ErrnoSnafu)?;
            }
        }
        Ok(())
    }
}

pub trait GetSymbol {
    fn get_sym(&self, name: &str) -> Option<*const ()>;
}

impl ELFLibrary {
    pub fn relocate_with<T>(
        self,
        raw_libs: &[&ELFLibrary],
        handle_libs: &[&ELFHandle],
        extern_libs: &[&T],
    ) -> Result<ELFHandle>
    where
        T: GetSymbol,
    {
        const REL_RELATIVE: u32 = R_X86_64_RELATIVE;
        const REL_GOT: u32 = R_X86_64_GLOB_DAT;
        const REL_DTPMOD: u32 = R_X86_64_DTPMOD64;
        const REL_SYMBOLIC: u32 = R_X86_64_64;
        const REL_IRELATIVE: u32 = R_X86_64_IRELATIVE;
        const REL_TPOFF: u32 = R_X86_64_TPOFF64;

        #[cfg(target_arch = "x86_64")]
        const REL_JUMP_SLOT: u32 = R_X86_64_JUMP_SLOT;
        #[cfg(target_arch = "x86")]
        const REL_JUMP_SLOT: u32 = R_X86_64_JUMP_SLOT;
        #[cfg(target_arch = "aarch64")]
        const REL_JUMP_SLOT: u32 = R_AARCH64_JUMP_SLOT;
        #[cfg(target_arch = "arm")]
        const REL_JUMP_SLOT: u32 = R_ARM_JUMP_SLOT;
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        const REL_JUMP_SLOT: u32 = R_RISCV_JUMP_SLOT;

        let pltrel = if let Some(pltrel) = self.relocation().pltrel {
            pltrel.iter()
        } else {
            [].iter()
        };

        let rela = if let Some(rela) = self.relocation().rel {
            rela.iter()
        } else {
            [].iter()
        };

        /*
            A Represents the addend used to compute the value of the relocatable field.
            B Represents the base address at which a shared object has been loaded into memory during execution.
            S Represents the value of the symbol whose index resides in the relocation entry.
        */

        // 因为REL_IRELATIVE的存在，对glibc来说rela和pltrel的重定位是有先后顺序的
        // 不过musl中没有出现过REL_IRELATIVE的重定位类型，我想这可能是libc实现的问题？
        for rela in rela.chain(pltrel) {
            let r_type = rela.r_info as usize & REL_MASK;
            match r_type as _ {
                // REL_GOT/REL_JUMP_SLOT: S  REL_SYMBOLIC: S + A
                REL_JUMP_SLOT | REL_GOT | REL_SYMBOLIC => {
                    let r_sym = rela.r_info as usize >> REL_BIT;
                    let dynsym = unsafe { self.symtab().add(r_sym).read() };
                    let append = rela.r_addend;
                    let symbol = if dynsym.st_info >> 4 == STB_LOCAL {
                        dynsym.st_value as _
                    } else {
                        let name = self
                            .strtab()
                            .get(dynsym.st_name as usize)
                            .map_err(parse_err_convert)?;

                        let symbol = BUILTIN
                            .get(&name)
                            .copied()
                            .or_else(|| {
                                if dynsym.st_shndx != SHN_UNDEF {
                                    return Some(unsafe {
                                        self.segments()
                                            .as_mut_ptr()
                                            .add(dynsym.st_value as usize)
                                            .cast()
                                    });
                                }

                                for lib in raw_libs {
                                    if let Some(sym) = lib.get_sym(name) {
                                        return Some(unsafe {
                                            self.segments()
                                                .as_mut_ptr()
                                                .add(sym.st_value as usize)
                                                .cast()
                                        });
                                    }
                                }

                                for lib in handle_libs {
                                    if let Some(sym) = lib.get_sym(name) {
                                        return Some(sym);
                                    }
                                }

                                for lib in extern_libs {
                                    if let Some(sym) = lib.get_sym(name) {
                                        return Some(sym);
                                    }
                                }

                                None
                            })
                            .ok_or_else(|| relocate_error(&name))?;
                        symbol
                    };

                    let rel_addr = unsafe {
                        self.segments()
                            .as_mut_ptr()
                            .add(rela.r_offset.checked_add_signed(append).unwrap() as usize)
                            as *mut usize
                    };

                    unsafe { rel_addr.write(symbol as usize) }
                }
                // B + A
                REL_RELATIVE => {
                    let rel_addr = unsafe {
                        self.segments().as_mut_ptr().add(rela.r_offset as usize) as *mut usize
                    };
                    unsafe { rel_addr.write(self.segments().base() + rela.r_addend as usize) }
                }
                // indirect( B + A )
                REL_IRELATIVE => {
                    let rel_addr = unsafe {
                        self.segments().as_mut_ptr().add(rela.r_offset as usize) as *mut usize
                    };
                    let ifunc: fn() -> usize = unsafe {
                        core::mem::transmute(self.segments().base() + rela.r_addend as usize)
                    };
                    unsafe { rel_addr.write(ifunc()) }
                }
                REL_TPOFF => {
                    // TODO:这个要根据TLS的实现来进行操作，可能还要覆盖glibc的一些符号
                    todo!("REL_TPOFF")
                }

                #[cfg(feature = "tls")]
                REL_DTPMOD => {
                    let rel_addr = unsafe {
                        self.segments().as_mut_ptr().add(rela.r_offset as usize) as *mut usize
                    };
                    unsafe {
                        rel_addr.write(self.tls().as_ref().unwrap().as_ref()
                            as *const crate::tls::ELFTLS
                            as usize)
                    }
                }
                _ => {
                    return elfloader_error("unsupport relocate type");
                }
            }

            #[cold]
            #[inline(never)]
            fn relocate_error(name: &str) -> crate::Error {
                Error::RelocateError {
                    msg: format!("can not relocate symbol {}", name),
                }
            }
        }

        if let Some(init) = self.init_fn() {
            init();
        }

        if let Some(init_array) = self.init_array_fn() {
            for init in *init_array {
                init();
            }
        }

        if let Some(relro) = &self.relocation().relro {
            relro.relro()?;
        }

        let mut inner_libs = vec![];
        inner_libs.extend(raw_libs.into_iter().map(|lib| lib.inner.clone()));
        inner_libs.extend(handle_libs.into_iter().map(|lib| lib.inner.inner.clone()));

        Ok(ELFHandle::new(self, inner_libs))
    }
}
