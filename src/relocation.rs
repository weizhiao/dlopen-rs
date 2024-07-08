use crate::{segment::ELFSegments, Phdr, Result, MASK, PAGE_SIZE};
use core::ptr::NonNull;
use elf::relocation::Rela;
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
