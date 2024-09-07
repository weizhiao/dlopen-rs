use std::ptr::NonNull;

use crate::Result;
use elf::abi::{PF_R, PF_W, PF_X};
use nix::sys::mman;

use super::{ELFRelro, ELFSegments, MASK, PAGE_SIZE};

impl ELFRelro {
    #[inline]
    pub(crate) fn relro(&self) -> Result<()> {
        let end = (self.addr + self.len + PAGE_SIZE - 1) & MASK;
        let start = self.addr & MASK;
        let start_addr = unsafe { NonNull::new_unchecked(start as _) };
        unsafe {
            mman::mprotect(start_addr, end - start, mman::ProtFlags::PROT_READ)?;
        }

        Ok(())
    }
}

impl Drop for ELFSegments {
    fn drop(&mut self) {
        unsafe {
            mman::munmap(self.memory, self.len).unwrap();
        }
    }
}

impl ELFSegments {
    #[inline]
    pub(crate) fn map_prot(prot: u32) -> nix::sys::mman::ProtFlags {
        use nix::sys::mman::ProtFlags;
        let mut prot_flag = ProtFlags::empty();
        if prot & PF_X != 0 {
            prot_flag |= ProtFlags::PROT_EXEC;
        }
        if prot & PF_W != 0 {
            prot_flag |= ProtFlags::PROT_WRITE;
        }
        if prot & PF_R != 0 {
            prot_flag |= ProtFlags::PROT_READ;
        }
        prot_flag
    }
}
