use crate::{
    elfloader_error, loader::ELFLibrary, parse_err_convert, segment::ELFSegments, Error, Phdr,
    Rela, Result, MASK, PAGE_SIZE, REL_BIT, REL_MASK,
};
use core::ptr::NonNull;
use elf::abi::*;
use snafu::ResultExt;

#[derive(Debug)]
pub(crate) struct ELFRelas {
    pub(crate) pltrel: &'static [Rela],
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
            addr: segments.base() + phdr.p_vaddr as usize - segments.addr_min(),
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

impl ELFLibrary {
    #[inline]
    pub fn relocate(&self, libs: &[ELFLibrary]) -> Result<()> {
        let pltrel = self.rela_sections.pltrel;
        for rela in pltrel {
            let r_type = rela.r_info as usize & REL_MASK;
            let r_sym = rela.r_info as usize >> REL_BIT;
            let dynsym = unsafe { self.symtab.add(r_sym).read() };
            let name = self
                .strtab
                .get(dynsym.st_name as usize)
                .map_err(parse_err_convert)?;

            #[cold]
            #[inline(never)]
            fn relocate_error(name: &str) -> Result<()> {
                Err(Error::RelocateError {
                    msg: format!("can not relocate symbol {}", name),
                })
            }

            let mut symbol = None;
            for lib in libs {
                symbol = lib.get(name);
            }

			if symbol.is_none(){
				return relocate_error(&name);
			}

            let rel_addr = unsafe {
                self.segments
                    .as_mut_ptr()
                    .add(rela.r_offset as usize - self.segments.addr_min())
                    as *mut usize
            };

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

            match r_type as _ {
                REL_JUMP_SLOT => unsafe { rel_addr.write(*symbol.unwrap().cast()) },
                _ => {
                    return elfloader_error("unsupport rela type");
                }
            }
        }

        if let Some(relro) = &self.relro {
            relro.relro()?;
        }

        Ok(())
    }
}
