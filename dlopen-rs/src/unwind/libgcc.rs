use std::ffi::c_void;

use crate::segment::ELFSegments;

use super::ELFUnwind;

impl Drop for ELFUnwind {
    fn drop(&mut self) {
        extern "C" {
            fn __deregister_frame(begin: *const c_void);
        }
        unsafe { __deregister_frame(self.0 as _) };
    }
}

impl ELFUnwind {
    #[inline]
    pub(crate) fn register_unwind(&self, _segments: &ELFSegments) {
        extern "C" {
            fn __register_frame(begin: *const c_void);
        }
        //在使用libgcc的情况下直接传eh_frame的地址即可
        unsafe { __register_frame(self.0 as _) };
    }
}
