use super::{
    arch::Dyn,
    dso::{dynamic::ELFRawDynamic, SymbolData},
    ELFLibrary, LibraryExtraData, RelocatedLibraryInner,
};
use crate::{find_lib_error, RelocatedLibrary, Result};
use core::{ffi::c_char, fmt::Debug, mem::MaybeUninit, sync::atomic::AtomicBool};
use nix::libc::{dlclose, dlinfo, dlopen, RTLD_DI_LINKMAP, RTLD_LOCAL, RTLD_NOW};
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

#[allow(unused)]
pub(crate) struct ExtraData {
    handle: *mut c_void,
}

impl Debug for ExtraData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ExtraData")
            .field("handler", &self.handle)
            .finish()
    }
}

impl Drop for ExtraData {
    fn drop(&mut self) {
        unsafe { dlclose(self.handle) };
    }
}

impl ELFLibrary {
    /// Convert a raw handle returned by `dlopen`-family of calls to a `RelocatedLibrary`.
    ///
    /// # Safety
    ///
    /// The pointer shall be a result of a successful call of the `dlopen`-family of functions. It must be valid to call `dlclose`
    /// with this pointer as an argument.
    pub unsafe fn sys_load_from_raw(handle: *mut c_void, name: &str) -> Result<RelocatedLibrary> {
        let cstr = CString::new(name).unwrap();
        if handle.is_null() {
            return Err(find_lib_error(format!(
                "{}: load fail",
                cstr.to_str().unwrap()
            )));
        }
        Self::sys_load_impl(handle, cstr)
    }

    ///Use the system dynamic linker (ld.so) to load the dynamic library, allowing it to be used in the same way as dlopen.
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    /// ```
    pub fn sys_load(name: &str) -> Result<RelocatedLibrary> {
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

    fn sys_load_impl(handle: *mut c_void, cstr: CString) -> Result<RelocatedLibrary> {
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
        let dynamic = ELFRawDynamic::new(link_map.l_ld)?;
        let base = if dynamic.hash_off() > link_map.l_addr as usize {
            0
        } else {
            link_map.l_addr as usize
        };
        let dynamic = dynamic.finish(base);
        #[cfg(feature = "version")]
        let version = dynamic.version_idx().map(|version_idx| {
            super::dso::version::ELFVersion::new(
                version_idx + base,
                dynamic
                    .verneed()
                    .map(|(off, num)| (off + link_map.l_addr as usize, num)),
                dynamic
                    .verdef()
                    .map(|(off, num)| (off + link_map.l_addr as usize, num)),
            )
        });
        #[cfg(feature = "debug")]
        let debug = unsafe {
            use super::debug::*;
            let debug = {
                dl_debug_init(
                    link_map.l_addr as usize,
                    link_map.l_name,
                    link_map.l_ld as usize,
                )
            };
            debug
        };
        #[cfg(feature = "tls")]
        let tls_module_id = unsafe {
            use nix::libc::RTLD_DI_TLS_MODID;
            let module_id: MaybeUninit<usize> = MaybeUninit::uninit();
            if dlinfo(handle, RTLD_DI_TLS_MODID, module_id.as_ptr() as _) != 0 {
                return Err(find_lib_error(format!(
                    "{}: get tls module id fail",
                    cstr.to_str().unwrap()
                )));
            }
            let module_id = module_id.assume_init();
            module_id
        };

        Ok(RelocatedLibrary {
            inner: Arc::new((
                AtomicBool::new(false),
                RelocatedLibraryInner {
                    name: cstr,
                    #[cfg(feature = "debug")]
                    link_map: debug,
                    #[cfg(feature = "tls")]
                    tls: Some(tls_module_id),
                    base: link_map.l_addr as _,
                    symbols: SymbolData {
                        hashtab: dynamic.hashtab(),
                        symtab: dynamic.symtab(),
                        strtab: dynamic.strtab(),
                        #[cfg(feature = "version")]
                        version,
                    },
                    extra: LibraryExtraData::External(ExtraData { handle }),
                },
            )),
        })
    }
}
