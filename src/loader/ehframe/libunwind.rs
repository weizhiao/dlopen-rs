use core::ffi::c_void;

use crate::{
    loader::{arch::Phdr, segment::ELFSegments},
    Result,
};
#[derive(Debug)]
pub(crate) struct EhFrame(usize);

impl EhFrame {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> Result<EhFrame> {
        let base = segments.base();
        let eh_frame_hdr_off = phdr.p_vaddr as usize;
        let eh_frame_hdr_size = phdr.p_memsz as usize;
        let bases =
            gimli::BaseAddresses::default().set_eh_frame_hdr((eh_frame_hdr_off + base) as _);
        let eh_frame_hdr = gimli::EhFrameHdr::new(
            &segments.as_mut_slice()[eh_frame_hdr_off..eh_frame_hdr_off + eh_frame_hdr_size],
            gimli::NativeEndian,
        )
        .parse(&bases, core::mem::size_of::<usize>() as _)?;
        let eh_frame_addr = match eh_frame_hdr.eh_frame_ptr() {
            gimli::Pointer::Direct(x) => x as usize,
            gimli::Pointer::Indirect(x) => unsafe { *(x as *const _) },
        };
        let unwind = EhFrame(eh_frame_addr);
        unwind.register_unwind(segments);
        Ok(unwind)
    }
}

impl Drop for EhFrame {
    fn drop(&mut self) {
        extern "C" {
            fn __deregister_frame(begin: *const c_void);
        }

        let mut current = self.0;
        let mut len = unsafe { core::ptr::read::<u32>(current as *const u32) } as u64;
        current += 4;
        if len == 0xFFFFFFFF {
            len = unsafe { core::ptr::read::<u64>(current as *const u64) };
            current += 8;
        }

        //跳过CIE
        current += len as usize;

        loop {
            let fde = current;
            len = unsafe { core::ptr::read::<u32>(current as *const u32) } as u64;
            current += 4;
            if len == 0xFFFFFFFF {
                len = unsafe { core::ptr::read::<u64>(current as *const u64) };
                current += 8;
            }
            if len == 0 {
                break;
            }
            unsafe { __deregister_frame(fde as _) };
            current += len as usize;
        }
    }
}

impl EhFrame {
    #[inline]
    pub(crate) fn register_unwind(&self, _segments: &ELFSegments) {
        extern "C" {
            fn __register_frame(begin: *const c_void);
        }

        unsafe {
            // 使用libunwind时__register_frame传入的参数只能是单个的fde
            let mut current = self.0;
            let mut len = core::ptr::read::<u32>(current as *const u32) as u64;
            current += 4;
            if len == 0xFFFFFFFF {
                len = core::ptr::read::<u64>(current as *const u64);
                current += 8;
            }

            //跳过CIE
            current += len as usize;

            loop {
                let fde = current;
                len = core::ptr::read::<u32>(current as *const u32) as u64;
                current += 4;
                if len == 0xFFFFFFFF {
                    len = core::ptr::read::<u64>(current as *const u64);
                    current += 8;
                }
                if len == 0 {
                    break;
                }
                __register_frame(fde as _);
                current += len as usize;
            }
        }
    }
}
