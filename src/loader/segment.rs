use super::mmap::{self, Mmap, ProtFlags};
use crate::{loader::arch::Phdr, Result};
use alloc::boxed::Box;
use core::ffi::c_void;
use core::fmt::Debug;
use core::ptr::NonNull;
use elf::abi::{PF_R, PF_W, PF_X};

#[cfg(target_arch = "aarch64")]
pub(crate) const PAGE_SIZE: usize = 0x10000;
#[cfg(not(target_arch = "aarch64"))]
pub(crate) const PAGE_SIZE: usize = 0x1000;

pub(crate) const MASK: usize = !(PAGE_SIZE - 1);

#[allow(unused)]
pub(crate) struct ELFRelro {
    addr: usize,
    len: usize,
    mprotect: Box<dyn Fn(NonNull<c_void>, usize, ProtFlags) -> Result<()>>,
}

impl ELFRelro {
    pub(crate) fn new<M: Mmap>(phdr: &Phdr, base: usize) -> ELFRelro {
        ELFRelro {
            addr: base + phdr.p_vaddr as usize,
            len: phdr.p_memsz as usize,
            mprotect: Box::new(|addr, len, prot| unsafe { M::mprotect(addr, len, prot) }),
        }
    }
}

pub(crate) struct ELFSegments {
    memory: NonNull<c_void>,
    /// -addr_min / -addr_min + align_offset
    offset: isize,
    len: usize,
    munmap: Box<dyn Fn(NonNull<c_void>, usize)>,
}

impl Debug for ELFSegments {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ELFSegments")
            .field("memory", &self.memory)
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

impl ELFRelro {
    #[inline]
    pub(crate) fn relro(&self) -> Result<()> {
        let end = (self.addr + self.len + PAGE_SIZE - 1) & MASK;
        let start = self.addr & MASK;
        let start_addr = unsafe { NonNull::new_unchecked(start as _) };
        (self.mprotect)(start_addr, end - start, ProtFlags::PROT_READ)?;
        Ok(())
    }
}

impl Drop for ELFSegments {
    fn drop(&mut self) {
        (self.munmap)(self.memory, self.len);
    }
}

impl ELFSegments {
    #[inline]
    pub(crate) fn map_prot(prot: u32) -> mmap::ProtFlags {
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

impl ELFSegments {
    pub(crate) fn new<M: Mmap>(memory: NonNull<c_void>, offset: isize, len: usize) -> Self {
        ELFSegments {
            memory,
            offset,
            len,
            munmap: Box::new(|addr, len: usize| unsafe {
                M::munmap(addr, len).unwrap();
            }),
        }
    }

    #[inline]
    #[allow(unused)]
    pub(crate) fn offset(&self) -> isize {
        self.offset
    }

    #[inline]
    #[allow(unused)]
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// base = memory_addr - addr_min
    #[inline]
    pub(crate) fn base(&self) -> usize {
        unsafe { self.memory.as_ptr().cast::<u8>().byte_offset(self.offset) as usize }
    }

    /// start = memory_addr - addr_min
    #[inline]
    pub(crate) fn as_mut_ptr(&self) -> *mut u8 {
        unsafe { self.memory.as_ptr().cast::<u8>().byte_offset(self.offset) }
    }

    /// start = memory_addr - addr_min
    #[inline]
    pub(crate) fn as_mut_slice(&self) -> &'static mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.memory.as_ptr().cast::<u8>().byte_offset(self.offset),
                self.len,
            )
        }
    }
}
