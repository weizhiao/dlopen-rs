use core::num::NonZeroUsize;

use nix::sys::mman;

use crate::{
    loader::{
        arch::Phdr,
        dso::{
            segment::{ELFSegments, MASK, PAGE_SIZE},
            MapSegment,
        },
    },
    unlikely,
};

use super::ELFFile;

impl MapSegment for ELFFile {
    fn create_segments(
        &self,
        addr_min: usize,
        size: usize,
        offset: usize,
        prot: u32,
    ) -> crate::Result<ELFSegments> {
        let memory = unsafe {
            mman::mmap(
                None,
                NonZeroUsize::new_unchecked(size),
                ELFSegments::map_prot(prot),
                mman::MapFlags::MAP_PRIVATE,
                &self.file,
                offset as _,
            )?
        };
        Ok(ELFSegments::new(memory, -(addr_min as isize), size))
    }

    fn load_segment(&mut self, segments: &ELFSegments, phdr: &Phdr) -> crate::Result<()> {
        // 映射的起始地址与结束地址都是页对齐的
        let addr_min = (-segments.offset()) as usize;
        let base = segments.base();
        // addr_min对应memory中的起始
        let this_min = phdr.p_vaddr as usize & MASK;
        let this_max = (phdr.p_vaddr as usize + phdr.p_memsz as usize + PAGE_SIZE - 1) & MASK;
        let this_len = NonZeroUsize::new(this_max - this_min).unwrap();
        let this_port = ELFSegments::map_prot(phdr.p_flags);
        let this_addr = NonZeroUsize::new(this_min + base).unwrap();
        let this_off = phdr.p_offset as usize & MASK;
        // 将类似bss节的内存区域的值设置为0
        if addr_min != this_min {
            let _ = unsafe {
                mman::mmap(
                    Some(this_addr),
                    this_len,
                    this_port,
                    mman::MapFlags::MAP_PRIVATE | mman::MapFlags::MAP_FIXED,
                    &self.file,
                    this_off as _,
                )?
            };
            //将类似bss节的内存区域的值设置为0
            if unlikely(phdr.p_filesz != phdr.p_memsz) {
                // 用0填充这一页
                let zero_start = (phdr.p_vaddr + phdr.p_filesz) as usize;
                let zero_end = (zero_start + PAGE_SIZE - 1) & MASK;
                let zero_mem = &mut segments.as_mut_slice()[zero_start..zero_end];
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
                        )?;
                    }
                }
            }
        }
        Ok(())
    }
}
