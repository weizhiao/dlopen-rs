use crate::Phdr;
use core::ffi::c_void;
use core::ptr::NonNull;

#[cfg(feature = "mmap")]
mod mmap;

#[cfg(not(feature = "mmap"))]
mod no_mmap;

#[cfg(target_arch = "aarch64")]
const PAGE_SIZE: usize = 0x10000;
#[cfg(not(target_arch = "aarch64"))]
const PAGE_SIZE: usize = 0x1000;

const MASK: usize = (0 - PAGE_SIZE as isize) as usize;

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
    pub(crate) memory: NonNull<c_void>,
	/// -addr_min / -addr_min + align_offset
    pub(crate) offset: isize,
    pub(crate) len: usize,
}

impl ELFSegments {
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

    #[cfg(feature = "unwinding")]
    #[inline]
    fn get_unwind_info(&self, phdr: &Phdr) -> Result<usize> {
        let addr_min = self.addr_min;
        let base = self.memory as usize;
        let eh_frame_addr = phdr.p_vaddr as usize - addr_min + base;
        Ok(eh_frame_addr)
    }
}
