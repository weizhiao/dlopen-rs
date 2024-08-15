use crate::{
    dso::{create_segments_impl, MapSegment},
    unlikely,
};

use super::ELFBinary;

impl MapSegment for ELFBinary<'_> {
    fn create_segments(
        &self,
        addr_min: usize,
        size: usize,
        _offset: usize,
        _prot: u32,
    ) -> crate::Result<crate::segment::ELFSegments> {
        create_segments_impl(addr_min, size)
    }

    fn load_segment(
        &mut self,
        segments: &crate::segment::ELFSegments,
        phdr: &crate::Phdr,
    ) -> crate::Result<()> {
        let memory_slice = segments.as_mut_slice();
        let this_min = phdr.p_vaddr as usize;
        let this_max = (phdr.p_vaddr + phdr.p_filesz) as usize;
        let this_off = phdr.p_offset as usize;
        let this_off_end = (phdr.p_offset + phdr.p_filesz) as usize;
        let this_mem = &mut memory_slice[this_min..this_max];

        this_mem.copy_from_slice(&self.bytes[this_off..this_off_end]);

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
