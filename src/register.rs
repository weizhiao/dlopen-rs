use crate::RelocatedLibrary;
use core::ptr::{self, null_mut};
use hashbrown::HashMap;
use std::{
    ffi::CString,
    sync::{Arc, LazyLock, RwLock},
};

static REGISTER_LIBS: LazyLock<RwLock<HashMap<CString, RelocatedLibrary>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

impl RelocatedLibrary {
    /// Registers the loaded dynamic library to ensure the correct execution
    /// of the `dl_iterate_phdr` function, allowing the use of the `backtrace`
    /// function within the loaded dynamic library.
    pub fn register(&self) -> Option<RelocatedLibrary> {
        let mut writer = REGISTER_LIBS.write().unwrap();
        self.set_register();
        writer.insert(self.name().to_owned(), self.clone())
    }
}

impl Drop for RelocatedLibrary {
    fn drop(&mut self) {
        if self.is_register() {
            if Arc::strong_count(&self.inner) == 2 {
                let mut writer = REGISTER_LIBS.write().unwrap();
                let remove = writer.remove(self.name());
                //防止死锁
                drop(writer);
                drop(remove);
            }
        }
    }
}

pub(crate) unsafe extern "C" fn dl_iterate_phdr_impl(
    callback: Option<
        unsafe extern "C" fn(
            info: *mut nix::libc::dl_phdr_info,
            size: nix::libc::size_t,
            data: *mut nix::libc::c_void,
        ) -> nix::libc::c_int,
    >,
    data: *mut nix::libc::c_void,
) -> nix::libc::c_int {
    use nix::libc::dl_phdr_info;
    let reader = REGISTER_LIBS.read().unwrap();
    let mut ret = nix::libc::dl_iterate_phdr(callback, data);
    for lib in reader.values().filter_map(|lib| lib.into_internal_lib()) {
        let (dlpi_phdr, dlpi_phnum) = lib
            .common_data()
            .phdrs()
            .map(|phdrs| (phdrs.as_ptr().cast(), phdrs.len() as _))
            .unwrap_or((ptr::null(), 0));
        let mut info = dl_phdr_info {
            dlpi_addr: lib.common_data().base() as _,
            dlpi_name: lib.name().as_ptr().cast(),
            dlpi_phdr,
            dlpi_phnum,
            dlpi_adds: reader.len() as _,
            dlpi_subs: 0,
            dlpi_tls_modid: 0,
            dlpi_tls_data: null_mut(),
        };
        if let Some(callback) = callback {
            ret = callback(&mut info, size_of::<dl_phdr_info>(), data);
            if ret != 0 {
                break;
            }
        }
    }
    ret
}
