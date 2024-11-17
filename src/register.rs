use crate::{
    loader::{ehframe::EhFrame, tls::ElfTls},
    ElfLibrary,
};
use core::{ffi::CStr, ptr::null_mut, sync::atomic::AtomicBool};
use elf_loader::{arch::Phdr, ElfDylib};
use hashbrown::HashMap;
use std::{
    ffi::CString,
    sync::{LazyLock, RwLock},
};

static REGISTER_LIBS: LazyLock<RwLock<HashMap<CString, &'static RegisterInfo>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

pub trait Register {
    fn register(self) -> Self;
}

pub(crate) struct RegisterInfo {
    name: &'static CStr,
    mark: AtomicBool,
    phdrs: &'static [Phdr],
    base: usize,
}

impl RegisterInfo {
    pub(crate) fn new(dylib: &ElfDylib<ElfTls, EhFrame>) -> Self {
        Self {
            name: unsafe { core::mem::transmute(dylib.cname()) },
            mark: AtomicBool::new(false),
            phdrs: unsafe { core::mem::transmute(dylib.phdrs()) },
            base: dylib.base(),
        }
    }
}

impl Register for ElfLibrary {
    fn register(self) -> Self {
        let mut writer = REGISTER_LIBS.write().unwrap();
        let info = self
            .dylib
            .user_data()
            .data()
            .iter()
            .rev()
            .find_map(|data| data.downcast_ref::<RegisterInfo>())
            .unwrap();
        info.mark.store(true, core::sync::atomic::Ordering::Relaxed);
        writer.insert(self.dylib.cname().to_owned(), unsafe {
            core::mem::transmute(info)
        });
        self
    }
}

impl Drop for RegisterInfo {
    fn drop(&mut self) {
        if self.mark.load(core::sync::atomic::Ordering::Relaxed) {
            let mut writer = REGISTER_LIBS.write().unwrap();
            writer.remove(self.name);
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
    for info in reader.values() {
        let mut info = dl_phdr_info {
            dlpi_addr: info.base as _,
            dlpi_name: info.name.as_ptr().cast(),
            dlpi_phdr: info.phdrs.as_ptr().cast(),
            dlpi_phnum: info.phdrs.len() as _,
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
