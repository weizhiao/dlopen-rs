use super::mmap::{self, Mmap, ProtFlags, RawData};
use crate::{loader::arch::Phdr, Result};
use alloc::boxed::Box;
use core::ffi::c_void;
use core::fmt::Debug;
use core::ptr::NonNull;
use elf::abi::{PF_R, PF_W, PF_X};

#[cfg(target_arch = "aarch64")]
pub const PAGE_SIZE: usize = 0x10000;
#[cfg(not(target_arch = "aarch64"))]
pub const PAGE_SIZE: usize = 0x1000;

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
    pub(crate) fn map_prot(prot: u32) -> mmap::ProtFlags {
        mmap::ProtFlags::from_bits_retain(
            ((prot & PF_X) << 2 | prot & PF_W | (prot & PF_R) >> 2) as _,
        )
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

pub(crate) trait MapSegment: RawData {
    fn create_segments<M: Mmap>(
        &self,
        min_vaddr: usize,
        total_size: usize,
        offset: usize,
        len: usize,
        prot: u32,
    ) -> crate::Result<ELFSegments> {
        let memory = unsafe {
            M::mmap(
                None,
                total_size,
                ELFSegments::map_prot(prot),
                mmap::MapFlags::MAP_PRIVATE,
                self.transport(offset, len),
            )?
        };
        Ok(ELFSegments::new::<M>(
            memory,
            -(min_vaddr as isize),
            total_size,
        ))
    }

    fn load_segment<M: Mmap>(&self, segments: &ELFSegments, phdr: &Phdr) -> crate::Result<()> {
        // 映射的起始地址与结束地址都是页对齐的
        let addr_min = (-segments.offset()) as usize;
        let base = segments.base();
        let min_vaddr = phdr.p_vaddr as usize & MASK;
        let max_vaddr = (phdr.p_vaddr as usize + phdr.p_memsz as usize + PAGE_SIZE - 1) & MASK;
        let memsz = max_vaddr - min_vaddr;
        let prot = ELFSegments::map_prot(phdr.p_flags);
        let real_addr = min_vaddr + base;
        let offset = phdr.p_offset as usize;
        let filesz = phdr.p_filesz as usize;
        // 将类似bss节的内存区域的值设置为0
        if addr_min != min_vaddr {
            let _ = unsafe {
                M::mmap(
                    Some(real_addr),
                    memsz,
                    prot,
                    mmap::MapFlags::MAP_PRIVATE | mmap::MapFlags::MAP_FIXED,
                    self.transport(offset, filesz),
                )?
            };
            //将类似bss节的内存区域的值设置为0
            if phdr.p_filesz != phdr.p_memsz {
                // 用0填充这一页
                let zero_start = (phdr.p_vaddr + phdr.p_filesz) as usize;
                let zero_end = (zero_start + PAGE_SIZE - 1) & MASK;
                let zero_mem = &mut segments.as_mut_slice()[zero_start..zero_end];
                zero_mem.fill(0);

                if zero_end < max_vaddr {
                    //之后剩余的一定是页的整数倍
                    //如果有剩余的页的话，将其映射为匿名页
                    let zero_mmap_addr = base + zero_end;
                    let zero_mmap_len = max_vaddr - zero_end;
                    unsafe {
                        M::mmap_anonymous(
                            zero_mmap_addr,
                            zero_mmap_len,
                            prot,
                            mmap::MapFlags::MAP_PRIVATE | mmap::MapFlags::MAP_FIXED,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}
