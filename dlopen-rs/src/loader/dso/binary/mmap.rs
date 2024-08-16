use core::{num::NonZeroUsize, ptr::NonNull};

use nix::sys::mman;

use crate::loader::{
    arch::Phdr,
    dso::{
        segment::{ELFSegments, MASK, PAGE_SIZE},
        MapSegment,
    },
};

use super::ELFBinary;

impl MapSegment for ELFBinary<'_> {
    fn create_segments(
        &self,
        addr_min: usize,
        size: usize,
        _offset: usize,
        _prot: u32,
    ) -> crate::Result<ELFSegments> {
        let memory = unsafe {
            mman::mmap_anonymous(
                None,
                NonZeroUsize::new_unchecked(size),
                mman::ProtFlags::PROT_WRITE,
                mman::MapFlags::MAP_PRIVATE | mman::MapFlags::MAP_ANON,
            )?
        };
        Ok(ELFSegments {
            memory,
            offset: -(addr_min as isize),
            len: size,
        })
    }

    fn load_segment(&mut self, segments: &ELFSegments, phdr: &Phdr) -> crate::Result<()> {
        // 映射的起始地址与结束地址都是页对齐的
        let addr_min = (-segments.offset) as usize;
        let base = segments.base();
        // addr_min对应memory中的起始
        let this_min = phdr.p_vaddr as usize & MASK;
        let this_max = (phdr.p_vaddr as usize + phdr.p_memsz as usize + PAGE_SIZE - 1) & MASK;
        let this_len = NonZeroUsize::new(this_max - this_min).unwrap();
        let this_port = ELFSegments::map_prot(phdr.p_flags);
        let this_addr = NonZeroUsize::new(this_min + base).unwrap();

        let this_off = phdr.p_offset as usize;
        let copy_start = phdr.p_vaddr as usize - addr_min;
        let copy_len = phdr.p_filesz as usize;
        let copy_end = copy_start + copy_len;
        let this_mem = &mut segments.as_mut_slice()[copy_start..copy_end];
        this_mem.copy_from_slice(&self.bytes[this_off..this_off + copy_len]);
        unsafe {
            mman::mprotect(
                NonNull::new_unchecked(this_addr.get() as _),
                this_len.get(),
                this_port,
            )?
        }
        Ok(())
    }
}
