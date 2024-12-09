use crate::{
    find_lib_error,
    loader::ElfLibrary,
    register::{register, RegisterInfo},
    Result,
};
use core::{ffi::c_char, mem::MaybeUninit, ptr::NonNull};
use elf_loader::{
    arch::Dyn, dynamic::ElfRawDynamic, segment::ElfSegments, Dylib, RelocatedDylib, UserData,
};
use libc::{dlclose, dlinfo, dlopen, RTLD_DI_LINKMAP, RTLD_LOCAL, RTLD_NOW};
use std::{
    ffi::{c_void, CString},
    sync::Arc,
};

#[repr(C)]
struct LinkMap {
    pub l_addr: *mut c_void,
    pub l_name: *const c_char,
    pub l_ld: *mut Dyn,
    l_next: *mut LinkMap,
    l_prev: *mut LinkMap,
}

impl ElfLibrary {
    /// Convert a raw handle returned by `dlopen`-family of calls to a `RelocatedLibrary`.
    ///
    /// # Safety
    ///
    /// The pointer shall be a result of a successful call of the `dlopen`-family of functions. It must be valid to call `dlclose`
    /// with this pointer as an argument.
    pub unsafe fn sys_load_from_raw(handle: *mut c_void, name: &str) -> Result<RelocatedDylib> {
        let cstr = CString::new(name).unwrap();
        if handle.is_null() {
            return Err(find_lib_error(format!(
                "{}: load fail",
                cstr.to_str().unwrap()
            )));
        }
        Self::sys_load_impl(handle, cstr)
    }

    /// Use the system dynamic linker (ld.so) to load the dynamic library, allowing it to be used in the same way as dlopen.
    /// # Examples
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    /// ```
    pub fn sys_load(name: &str) -> Result<RelocatedDylib> {
        let cstr = CString::new(name).unwrap();
        let handle = unsafe { dlopen(cstr.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if handle.is_null() {
            return Err(find_lib_error(format!(
                "{}: load fail",
                cstr.to_str().unwrap()
            )));
        }
        Self::sys_load_impl(handle, cstr)
    }

    fn sys_load_impl(handle: *mut c_void, cstr: CString) -> Result<RelocatedDylib> {
        let link_map = unsafe {
            let link_map: MaybeUninit<*const LinkMap> = MaybeUninit::uninit();
            if dlinfo(handle, RTLD_DI_LINKMAP, link_map.as_ptr() as _) != 0 {
                return Err(find_lib_error(format!(
                    "{}: get debug info fail",
                    cstr.to_str().unwrap()
                )));
            }
            &*link_map.assume_init()
        };
        let dynamic = ElfRawDynamic::new(link_map.l_ld)?;
        let base = if dynamic.hash_off > link_map.l_addr as usize {
            0
        } else {
            link_map.l_addr as usize
        };
        #[allow(unused_mut)]
        let mut dynamic = dynamic.finish(base);
        let mut user_data = UserData::empty();
        user_data.data_mut().push(Box::new(RegisterInfo {
            name: cstr.to_str().unwrap().to_string(),
            phdrs: None,
            base: link_map.l_addr as usize,
        }));
        #[cfg(feature = "debug")]
        unsafe {
            use super::debug::*;
            let debug = DebugInfo::new(
                link_map.l_addr as usize,
                link_map.l_name as _,
                link_map.l_ld as usize,
            );
            user_data.data_mut().push(Box::new(debug));
        };
        #[cfg(feature = "tls")]
        let tls_module_id = unsafe {
            use libc::RTLD_DI_TLS_MODID;
            let module_id: MaybeUninit<usize> = MaybeUninit::uninit();
            if dlinfo(handle, RTLD_DI_TLS_MODID, module_id.as_ptr() as _) != 0 {
                return Err(find_lib_error(format!(
                    "{}: get tls module id fail",
                    cstr.to_str().unwrap()
                )));
            }
            let module_id = module_id.assume_init();
            Some(module_id)
        };
        #[cfg(not(feature = "tls"))]
        let tls_module_id = None;
        #[cfg(feature = "version")]
        {
            dynamic.verneed = dynamic
                .verneed
                .map(|(off, num)| (off + (link_map.l_addr as usize - base), num));
            dynamic.verdef = dynamic
                .verdef
                .map(|(off, num)| (off + (link_map.l_addr as usize - base), num));
        }
        unsafe fn drop_handle(handle: NonNull<c_void>, _len: usize) -> elf_loader::Result<()> {
            dlclose(handle.as_ptr());
            Ok(())
        }
        let ptr = unsafe { NonNull::new_unchecked(handle) };
        let segments = ElfSegments::new(ptr, 0, drop_handle);
        let mut dylib = RelocatedDylib {
            inner: Arc::new(unsafe {
                Dylib::from_raw(
                    cstr,
                    link_map.l_addr as usize,
                    dynamic,
                    tls_module_id,
                    segments,
                    user_data,
                )
            }),
        };
        register(&mut dylib);
        Ok(dylib)
    }
}
