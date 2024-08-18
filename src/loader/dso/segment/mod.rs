use core::ffi::c_void;
use core::ptr::NonNull;

use crate::loader::arch::Phdr;

#[cfg(feature = "mmap")]
mod mmap;

#[cfg(not(feature = "mmap"))]
mod no_mmap;

#[cfg(target_arch = "aarch64")]
pub(crate) const PAGE_SIZE: usize = 0x10000;
#[cfg(not(target_arch = "aarch64"))]
pub(crate) const PAGE_SIZE: usize = 0x1000;

pub(crate) const MASK: usize = (0 - PAGE_SIZE as isize) as usize;

#[cfg(not(feature = "mmap"))]
pub(crate) const ALIGN: usize = 8;

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
}

#[derive(Debug)]
pub(crate) struct ELFSegments {
    memory: NonNull<c_void>,
    /// -addr_min / -addr_min + align_offset
    offset: isize,
    len: usize,
}

impl ELFSegments {
    pub(crate) const fn new(memory: NonNull<c_void>, offset: isize, len: usize) -> ELFSegments {
        ELFSegments {
            memory,
            offset,
            len,
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
