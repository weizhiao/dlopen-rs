use super::ELFUnwind;

impl Drop for ELFUnwind {
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

impl ELFUnwind {
	#[inline]
    unsafe fn register_unwind_info(unwind_info: &ELFUnwind) {
        extern "C" {
            fn __register_frame(begin: *const c_void);
        }

        // 使用libunwind时__register_frame传入的参数只能是单个的fde
        let mut current = unwind_info.0;
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
