use crate::{file::FileType, unlikely, Phdr, Result};
use core::ffi::c_void;
use core::ptr::NonNull;

use elf::abi::{PF_R, PF_W, PF_X, PT_LOAD};
use snafu::ResultExt;

use crate::file::ELFFile;

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

    #[inline]
    pub(crate) fn relro(&self) -> Result<()> {
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

#[derive(Debug)]
pub(crate) struct ELFSegments {
    memory: NonNull<c_void>,
    addr_min: usize,
    len: usize,
}

#[cfg(not(feature = "mmap"))]
impl Drop for ELFSegments {
    fn drop(&mut self) {
        MEM_USED.store(false, core::sync::atomic::Ordering::SeqCst)
    }
}

#[cfg(feature = "mmap")]
impl Drop for ELFSegments {
    fn drop(&mut self) {
        use nix::sys::mman;
        unsafe {
            mman::munmap(self.memory, self.len).unwrap();
        }
    }
}

impl ELFSegments {
    /// base = memory_addr - addr_min
    #[inline]
    pub(crate) fn base(&self) -> usize {
        (self.memory.as_ptr()) as usize - self.addr_min
    }

    /// start = memory_addr - addr_min
    #[inline]
    pub(crate) fn as_mut_ptr(&self) -> *mut u8 {
        unsafe { self.memory.as_ptr().cast::<u8>().sub(self.addr_min) }
    }

    /// start = memory_addr - addr_min
    #[inline]
    pub(crate) fn as_mut_slice(&self) -> &'static mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.memory.as_ptr().cast::<u8>().sub(self.addr_min),
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

#[cfg(feature = "mmap")]
impl ELFSegments {
    #[inline]
    fn map_prot(prot: u32) -> nix::sys::mman::ProtFlags {
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

    #[inline]
    pub(crate) fn new(phdrs: &[Phdr], file: &ELFFile) -> Result<ELFSegments> {
        use crate::ErrnoSnafu;
        use core::num::NonZeroUsize;
        use nix::sys::mman;

        let mut addr_min = usize::MAX;
        let mut addr_max = 0;
        let mut addr_min_off = 0;
        let mut addr_min_prot = 0;

        for phdr in phdrs {
            if phdr.p_type == PT_LOAD {
                let addr_start = phdr.p_vaddr as usize;
                let addr_end = (phdr.p_vaddr + phdr.p_memsz) as usize;
                if addr_start < addr_min {
                    addr_min = addr_start;
                    addr_min_off = phdr.p_offset as usize;
                    addr_min_prot = phdr.p_flags;
                }
                if addr_end > addr_max {
                    addr_max = addr_end;
                }
            }
        }

        addr_max += PAGE_SIZE - 1;
        addr_max &= MASK;
        addr_min &= MASK as usize;
        addr_min_off &= MASK;

        let len = addr_max - addr_min;

        let len = NonZeroUsize::new(len).unwrap();
        let memory = match &file.context {
            FileType::Fd(file) => unsafe {
                mman::mmap(
                    None,
                    len,
                    ELFSegments::map_prot(addr_min_prot),
                    mman::MapFlags::MAP_PRIVATE,
                    file,
                    addr_min_off as _,
                )
                .context(ErrnoSnafu)?
            },
            FileType::Binary(_) => unsafe {
                mman::mmap_anonymous(
                    None,
                    len,
                    mman::ProtFlags::PROT_WRITE,
                    mman::MapFlags::MAP_PRIVATE | mman::MapFlags::MAP_ANON,
                )
                .context(ErrnoSnafu)?
            },
        } as _;
        Ok(ELFSegments {
            memory,
            addr_min,
            len: len.get(),
        })
    }

    #[inline]
    pub(crate) fn load_segment(&self, phdr: &Phdr, file: &ELFFile) -> Result<()> {
        use crate::ErrnoSnafu;
        use core::num::NonZeroUsize;
        use nix::sys::mman;

        // 映射的起始地址与结束地址都是页对齐的
        let addr_min = self.addr_min;
        let base = self.base();
        // addr_min对应memory中的起始
        let this_min = phdr.p_vaddr as usize & MASK;
        let this_max = (phdr.p_vaddr as usize + phdr.p_memsz as usize + PAGE_SIZE - 1) & MASK;
        let this_len = NonZeroUsize::new(this_max - this_min).unwrap();
        let this_port = ELFSegments::map_prot(phdr.p_flags);
        let this_addr = NonZeroUsize::new(this_min + base).unwrap();

        match &file.context {
            FileType::Fd(file) => {
                let this_off = phdr.p_offset as usize & MASK;
                // 将类似bss节的内存区域的值设置为0
                if addr_min != this_min {
                    let _ = unsafe {
                        mman::mmap(
                            Some(this_addr),
                            this_len,
                            this_port,
                            mman::MapFlags::MAP_PRIVATE | mman::MapFlags::MAP_FIXED,
                            file,
                            this_off as _,
                        )
                        .context(ErrnoSnafu)?
                    };
                    //将类似bss节的内存区域的值设置为0
                    if unlikely(phdr.p_filesz != phdr.p_memsz) {
                        // 用0填充这一页
                        let zero_start = (phdr.p_vaddr + phdr.p_filesz) as usize;
                        let zero_end = (zero_start + PAGE_SIZE - 1) & MASK;
                        let zero_mem = &mut self.as_mut_slice()[zero_start..zero_end];
                        zero_mem.fill(0);

                        if zero_end < this_max {
                            //之后剩余的一定是页的整数倍
                            //如果有剩余的页的话，将其映射为匿名页
                            let zero_mmap_addr = NonZeroUsize::new(base + zero_end);
                            let zero_mmap_len = NonZeroUsize::new(this_max - zero_end).unwrap();
                            unsafe {
                                mman::mmap_anonymous(
                                    zero_mmap_addr,
                                    zero_mmap_len,
                                    this_port,
                                    mman::MapFlags::MAP_PRIVATE | mman::MapFlags::MAP_FIXED,
                                )
                                .context(ErrnoSnafu)?;
                            }
                        }

                        // 下面这段代码在加载libc时会遇到Bus error，我目前还不知道为什么，因此只能采用musl中的方式
                        // let zero_start = (phdr.p_vaddr + phdr.p_filesz) as usize;
                        // let zero_end = zero_start + (phdr.p_memsz - phdr.p_filesz) as usize;
                        // let zero_mem = &mut self.as_mut_slice()[zero_start..zero_end];
                        // zero_mem.fill(0);
                    }
                }
            }
            FileType::Binary(file) => {
                let this_off = phdr.p_offset as usize;
                let copy_start = phdr.p_vaddr as usize - addr_min;
                let copy_len = phdr.p_filesz as usize;
                let copy_end = copy_start + copy_len;
                let this_mem = &mut self.as_mut_slice()[copy_start..copy_end];
                this_mem.copy_from_slice(&file[this_off..this_off + copy_len]);
                unsafe {
                    mman::mprotect(
                        NonNull::new_unchecked(this_addr.get() as _),
                        this_len.get(),
                        this_port,
                    )
                    .context(ErrnoSnafu)?
                }
            }
        }
        Ok(())
    }
}

#[cfg(not(feature = "mmap"))]
static MEM_USED: AtomicBool = AtomicBool::new(false);

#[cfg(not(feature = "mmap"))]
impl ELFSegments {
    #[inline]
    fn new(
        prot: u32,
        len: usize,
        _off: usize,
        addr_min: usize,
        file: &ELFFile,
    ) -> Result<ELFSegments> {
        extern "C" {
            static mut __elfloader_memory_start: u8;
            static mut __elfloader_memory_end: u8;
        }

        if unlikely(MEM_USED.fetch_or(true, core::sync::atomic::Ordering::SeqCst)) {
            return elfloader_error("elfloader memory has been used");
        }

        let max_len = unsafe {
            &__elfloader_memory_end as *const u8 as isize
                - &__elfloader_memory_start as *const u8 as isize
        };

        if unlikely(max_len < len as isize) {
            return elfloader_error("elfloader memory overflow");
        }

        let memory = unsafe { &mut __elfloader_memory_start as *mut u8 };
        Ok(ELFSegments {
            memory,
            addr_min,
            len,
        })
    }

    #[inline]
    fn load_segment(&self, phdr: &Phdr, file: &mut ELFFile) -> Result<()> {
        let addr_min = self.addr_min;
        let memory_slice = self.as_mut_slice();
        let this_min = phdr.p_vaddr as usize - addr_min;
        let this_max = (phdr.p_vaddr + phdr.p_filesz) as usize - addr_min;
        let this_off = phdr.p_offset as usize;
        let this_off_end = (phdr.p_offset + phdr.p_filesz) as usize;
        let this_mem = &mut memory_slice[this_min..this_max];
        match &mut file.context {
            #[cfg(feature = "std")]
            Context::Fd(file) => {
                use crate::IOSnafu;
                use std::io::{Read, Seek, SeekFrom};
                file.seek(SeekFrom::Start(this_off.try_into().unwrap()))
                    .context(IOSnafu)?;
                file.read_exact(this_mem).context(IOSnafu)?;
            }
            Context::Binary(file) => {
                this_mem.copy_from_slice(&file[this_off..this_off_end]);
            }
        }
        //将类似bss节的内存区域的值设置为0
        if unlikely(phdr.p_filesz != phdr.p_memsz) {
            let zero_start = this_max;
            let zero_end = zero_start + (phdr.p_memsz - phdr.p_filesz) as usize;
            let zero_mem = &mut memory_slice[zero_start..zero_end];
            zero_mem.fill(0);
        }
        Ok(())
    }
}
