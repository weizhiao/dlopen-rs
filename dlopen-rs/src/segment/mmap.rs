use std::ptr::NonNull;

use elf::abi::{PF_R, PF_W, PF_X, PT_LOAD};
use snafu::ResultExt;

use crate::{
    file::{ELFFile, FileType},
    segment::{MASK, PAGE_SIZE},
    unlikely, Phdr, Result,
};

use super::{ELFRelro, ELFSegments};

impl ELFRelro {
    #[inline]
    pub(crate) fn relro(&self) -> Result<()> {
        use crate::ErrnoSnafu;
        use nix::sys::mman;
        let end = (self.addr + self.len + PAGE_SIZE - 1) & MASK;
        let start = self.addr & MASK;
        let start_addr = unsafe { NonNull::new_unchecked(start as _) };
        unsafe {
            mman::mprotect(start_addr, end - start, mman::ProtFlags::PROT_READ)
                .context(ErrnoSnafu)?;
        }

        Ok(())
    }
}

impl Drop for ELFSegments {
    fn drop(&mut self) {
        use nix::sys::mman;
        if self.len != isize::MAX as _ {
            unsafe {
                mman::munmap(self.memory, self.len).unwrap();
            }
        }
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
            offset: -(addr_min as isize),
            len: len.get(),
        })
    }

    #[inline]
    pub(crate) fn load_segment(&self, phdr: &Phdr, file: &ELFFile) -> Result<()> {
        use crate::ErrnoSnafu;
        use core::num::NonZeroUsize;
        use nix::sys::mman;

        // 映射的起始地址与结束地址都是页对齐的
        let addr_min = (-self.offset) as usize;
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
