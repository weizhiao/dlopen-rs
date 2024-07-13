use crate::{
    elfloader_error, loader::ELFLibrary, parse_err_convert, segment::ELFSegments, Error, Phdr,
    Rela, Result, MASK, PAGE_SIZE, REL_BIT, REL_MASK,
};
use core::ptr::NonNull;
use elf::abi::*;
use snafu::ResultExt;

#[derive(Debug)]
pub(crate) struct ELFRelas {
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
    fn get_sym(&self, name: &str) -> Option<&*const ()>;
}

impl GetSymbol for ELFLibrary {
    fn get_sym(&self, name: &str) -> Option<&*const ()> {
        let bytes = name.as_bytes();
        let name = if *bytes.last().unwrap() == 0 {
            &bytes[..bytes.len() - 1]
        } else {
            bytes
        };
        let symbol = unsafe { self.hashtab.find(name, self.symtab, &self.strtab) };
        if let Some(sym) = symbol {
            return Some(unsafe {
                core::mem::transmute(self.segments.as_mut_ptr().add(sym.st_value as usize))
            });
        }
        None
    }
}

impl ELFLibrary {
    // FIXME:dyn may cause performance degradation
    pub fn relocate_with(&self, libs: &[&dyn GetSymbol]) -> Result<()> {
        const REL_RELATIVE: u32 = R_X86_64_RELATIVE;
        const REL_GOT: u32 = R_X86_64_GLOB_DAT;

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

        let pltrel = if let Some(pltrel) = self.rela_sections.pltrel {
            pltrel.iter()
        } else {
            [].iter()
        };

        let rela = if let Some(rela) = self.rela_sections.rel {
            rela.iter()
        } else {
            [].iter()
        };

        /*  A Represents the addend used to compute the value of the relocatable field.
            B Represents the base address at which a shared object has been loaded into memory during execution.
            S Represents the value of the symbol whose index resides in the relocation entry.
        */

        for rela in pltrel.chain(rela) {
            let r_type = rela.r_info as usize & REL_MASK;
            match r_type as _ {
                // S
                REL_JUMP_SLOT | REL_GOT => {
                    let r_sym = rela.r_info as usize >> REL_BIT;
                    let dynsym = unsafe { self.symtab.add(r_sym).read() };
                    let name = self
                        .strtab
                        .get(dynsym.st_name as usize)
                        .map_err(parse_err_convert)?;
                    let mut symbol = None;
                    for lib in libs {
                        if let Some(sym) = lib.get_sym(name) {
                            symbol = Some(sym);
							break;
                        }
                    }

                    if symbol.is_none() {
                        return relocate_error(&name);
                    }

                    let rel_addr = unsafe {
                        self.segments.as_mut_ptr().add(rela.r_offset as usize) as *mut usize
                    };

                    unsafe { rel_addr.write(*symbol.unwrap() as usize) }
                }
                // B + A
                REL_RELATIVE => {
                    let rel_addr = unsafe {
                        self.segments.as_mut_ptr().add(rela.r_offset as usize) as *mut usize
                    };
                    unsafe { rel_addr.write(self.segments.base() + rela.r_addend as usize) }
                }
                _ => {
                    return elfloader_error("unsupport rela type");
                }
            }

            #[cold]
            #[inline(never)]
            fn relocate_error(name: &str) -> Result<()> {
                Err(Error::RelocateError {
                    msg: format!("can not relocate symbol {}", name),
                })
            }
        }

        if let Some(init) = self.init_fn {
            init();
        }

        if let Some(init_array) = self.init_array_fn {
            for init in init_array {
                init();
            }
        }

        if let Some(relro) = &self.rela_sections.relro {
            relro.relro()?;
        }

        Ok(())
    }
}
