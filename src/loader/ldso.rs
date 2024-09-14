use core::{ffi::CStr, fmt::Debug, sync::atomic::AtomicBool};
use std::{
    ffi::{c_void, CString},
    sync::Arc,
};

use nix::libc::{dlclose, dlopen, dlsym, RTLD_LOCAL, RTLD_NOW};

use crate::{find_lib_error, RelocatedLibrary, Result};

use super::{ELFLibrary, RelocatedLibraryInner};

#[allow(unused)]
pub(crate) struct ExternalLib {
    name: CString,
    handler: *mut c_void,
    #[cfg(feature = "debug")]
    debug: super::debug::DebugInfo,
}

impl Debug for ExternalLib {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExternalLib")
            .field("name", &self.name)
            .field("handler", &self.handler)
            .finish()
    }
}

impl Drop for ExternalLib {
    fn drop(&mut self) {
        unsafe { dlclose(self.handler) };
    }
}

impl ExternalLib {
    pub(crate) fn get_sym(&self, name: &CStr) -> Option<*const ()> {
        let sym = unsafe { dlsym(self.handler, name.as_ptr()) };
        if sym.is_null() {
            None
        } else {
            Some(sym.cast())
        }
    }

    pub(crate) fn name(&self) -> &CStr {
        &self.name
    }
}

impl ELFLibrary {
    ///Use the system dynamic linker (ld.so) to load the dynamic library, allowing it to be used in the same way as dlopen.
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    /// ```
    pub fn sys_load(name: &str) -> Result<RelocatedLibrary> {
        let cstr = CString::new(name).unwrap();
        let handler = unsafe { dlopen(cstr.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if handler.is_null() {
            return Err(find_lib_error(format!(
                "load {} fail",
                cstr.to_str().unwrap()
            )));
        }
        #[cfg(feature = "debug")]
        let debug = unsafe {
            use super::debug::*;
            use core::mem::MaybeUninit;
            use nix::libc::{dlinfo, RTLD_DI_LINKMAP};
            let debug = {
                let link_map: MaybeUninit<*const LinkMap> = MaybeUninit::uninit();
                dlinfo(handler, RTLD_DI_LINKMAP, link_map.as_ptr() as _);
                let link_map = &*link_map.assume_init();
                dl_debug_init(
                    link_map.l_addr as usize,
                    link_map.l_name,
                    link_map.l_ld as usize,
                )
            };
            dl_debug_finish();
            debug
        };
        Ok(RelocatedLibrary {
            inner: Arc::new((
                AtomicBool::new(false),
                RelocatedLibraryInner::External(ExternalLib {
                    name: cstr,
                    handler,
                    #[cfg(feature = "debug")]
                    debug,
                }),
            )),
        })
    }
}
