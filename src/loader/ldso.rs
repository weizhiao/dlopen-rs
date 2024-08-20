use core::{ffi::CStr, ptr::null};
use std::{
    ffi::{c_void, CString},
    sync::Arc,
};

use nix::libc::{dlclose, dlopen, dlsym, RTLD_LOCAL, RTLD_NOW};

use crate::{find_lib_error, RelocatedLibrary, Result};

use super::{ELFLibrary, RelocatedLibraryInner};

#[derive(Debug)]
pub(crate) struct ExternalLib {
    name: CString,
    handler: *mut c_void,
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
    /// Returns the handle to the main program
    pub fn load_self() -> Result<RelocatedLibrary> {
        let handler = unsafe { dlopen(null(), RTLD_NOW | RTLD_LOCAL) };
        if handler.is_null() {
            return Err(find_lib_error("get main program handler fail"));
        }
        Ok(RelocatedLibrary {
            inner: Arc::new(RelocatedLibraryInner::External(ExternalLib {
                name: c"main".to_owned(),
                handler,
            })),
        })
    }

    /// Use system dynamic linker(ldso) to load the dynamic library, you can use it in the same way as dlopen
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
        Ok(RelocatedLibrary {
            inner: Arc::new(RelocatedLibraryInner::External(ExternalLib {
                name: cstr,
                handler,
            })),
        })
    }
}
