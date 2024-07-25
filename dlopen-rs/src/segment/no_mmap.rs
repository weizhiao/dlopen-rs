use core::{alloc::Layout, ptr::NonNull};

use alloc::alloc::dealloc;
use elf::abi::PT_LOAD;

use crate::{
    file::{ELFFile, FileType},
    layout_err_convert,
    segment::{MASK, PAGE_SIZE},
    unlikely, Phdr, Result,
};

const ALIGN: usize = 8;

use super::{ELFRelro, ELFSegments};

impl Drop for ELFSegments {
    fn drop(&mut self) {
        if self.len != isize::MAX as usize {
            unsafe {
                dealloc(
                    self.memory.as_ptr() as _,
                    Layout::from_size_align_unchecked(self.len, ALIGN),
                )
            }
        }
    }
}

impl ELFRelro {
    #[inline]
    pub(crate) fn relro(&self) -> Result<()> {
        Ok(())
    }
}

impl ELFSegments {
    #[inline]
    pub(crate) fn new(phdrs: &[Phdr], _file: &ELFFile) -> Result<ELFSegments> {
        let mut addr_min = usize::MAX;
        let mut addr_max = 0;

        for phdr in phdrs {
            if phdr.p_type == PT_LOAD {
                let addr_start = phdr.p_vaddr as usize;
                let addr_end = (phdr.p_vaddr + phdr.p_memsz) as usize;
                if addr_start < addr_min {
                    addr_min = addr_start;
                }
                if addr_end > addr_max {
                    addr_max = addr_end;
                }
            }
        }

        addr_max += PAGE_SIZE - 1;
        addr_max &= MASK;
        addr_min &= MASK as usize;

        let len = addr_max - addr_min + PAGE_SIZE;

        // 鉴于有些平台无法分配出PAGE_SIZE对齐的内存，因此这里分配大一些的内存手动对齐
        let layout = Layout::from_size_align(len, ALIGN).map_err(layout_err_convert)?;
        let memory = unsafe { alloc::alloc::alloc(layout) };
        let align_offset = ((memory as usize + PAGE_SIZE - 1) & MASK) - memory as usize;

        let offset = align_offset as isize - addr_min as isize;

        // use this set prot to test no_mmap
        // unsafe {
        //     nix::sys::mman::mprotect(
        //         std::ptr::NonNull::new_unchecked(memory.byte_offset(offset) as _),
        //         len,
        //         nix::sys::mman::ProtFlags::PROT_EXEC
        //             | nix::sys::mman::ProtFlags::PROT_READ
        //             | nix::sys::mman::ProtFlags::PROT_WRITE,
        //     )
        //     .unwrap()
        // };

        let memory = unsafe { NonNull::new_unchecked(memory as _) };
        Ok(ELFSegments {
            memory,
            offset,
            len,
        })
    }

    #[inline]
    pub(crate) fn load_segment(&self, phdr: &Phdr, file: &mut ELFFile) -> Result<()> {
        let memory_slice = self.as_mut_slice();
        let this_min = phdr.p_vaddr as usize;
        let this_max = (phdr.p_vaddr + phdr.p_filesz) as usize;
        let this_off = phdr.p_offset as usize;
        let this_off_end = (phdr.p_offset + phdr.p_filesz) as usize;
        let this_mem = &mut memory_slice[this_min..this_max];
        match &mut file.context {
            #[cfg(feature = "std")]
            FileType::Fd(file) => {
                use std::io::{Read, Seek, SeekFrom};
                file.seek(SeekFrom::Start(this_off.try_into().unwrap()))
                    .map_err(io_err_convert)?;
                file.read_exact(this_mem).map_err(io_err_convert)?;
            }
            FileType::Binary(file) => {
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
