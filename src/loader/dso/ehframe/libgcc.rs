use core::ffi::c_void;

use crate::{
    loader::{arch::Phdr, dso::segment::ELFSegments},
    Result,
};

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
        Ok(EhFrame(eh_frame_addr))
    }
}

impl Drop for EhFrame {
    fn drop(&mut self) {
        extern "C" {
            fn __deregister_frame(begin: *const c_void);
        }
        unsafe { __deregister_frame(self.0 as _) };
    }
}

impl EhFrame {
    #[inline]
    pub(crate) fn register_unwind(&self, _segments: &ELFSegments) {
        extern "C" {
            fn __register_frame(begin: *const c_void);
        }
        //在使用libgcc的情况下直接传eh_frame的地址即可
        unsafe { __register_frame(self.0 as _) };
    }
}
