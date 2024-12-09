use crate::ElfLibrary;
use alloc::{
    borrow::ToOwned,
    string::String,
    sync::{Arc, Weak},
};
use elf_loader::{Dylib, RelocatedDylib};
use hashbrown::HashMap;
use spin::{Lazy, RwLock};

pub(crate) struct GlobalDylib {
    pub(crate) inner: Weak<Dylib>,
}

unsafe impl Send for GlobalDylib {}
unsafe impl Sync for GlobalDylib {}

pub(crate) static REGISTER_LIBS: Lazy<RwLock<HashMap<String, GlobalDylib>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub(crate) struct RegisterInfo {
    pub name: String,
    #[cfg(feature = "std")]
    pub phdrs: Option<&'static [elf_loader::arch::Phdr]>,
    #[cfg(feature = "std")]
    pub base: usize,
}

impl RegisterInfo {
    pub(crate) fn new(lib: &ElfLibrary) -> Self {
        Self {
            name: lib.dylib.name().to_owned(),
            #[cfg(feature = "std")]
            phdrs: Some(unsafe { core::mem::transmute(lib.dylib.phdrs()) }),
            #[cfg(feature = "std")]
            base: lib.dylib.base(),
        }
    }
}

pub(crate) fn register(dylib: &mut RelocatedDylib) {
    let info = if let Some(Some(info)) = dylib
        .user_data()
        .data()
        .iter()
        .next()
        .map(|data| data.downcast_ref::<RegisterInfo>())
    {
        info
    } else {
        return;
    };
    let global = GlobalDylib {
        inner: Arc::downgrade(&dylib.inner),
    };
    let mut writer = REGISTER_LIBS.write();
    writer.insert(info.name.clone(), global);
}

impl ElfLibrary {
    /// Register dynamic libraries as globally visible. When loading a dynamic library with `dlopen`,
    /// it looks up the globally visible dynamic library to see if the dynamic library being loaded already exists
    pub fn register(mut self) -> Self {
        let info = RegisterInfo::new(&self);
        unsafe {
            self.dylib
                .user_data_mut()
                .data_mut()
                .push(alloc::boxed::Box::new(info));
        }
        self
    }
}

impl Drop for RegisterInfo {
    fn drop(&mut self) {
        let mut writer = REGISTER_LIBS.write();
        writer.remove(&self.name);
    }
}

#[cfg(feature = "std")]
pub(crate) unsafe extern "C" fn dl_iterate_phdr_impl(
    callback: Option<
        unsafe extern "C" fn(
            info: *mut libc::dl_phdr_info,
            size: libc::size_t,
            data: *mut libc::c_void,
        ) -> libc::c_int,
    >,
    data: *mut libc::c_void,
) -> libc::c_int {
    use libc::dl_phdr_info;
    let reader = REGISTER_LIBS.read();
    let mut ret = libc::dl_iterate_phdr(callback, data);
    for lib in reader.values() {
        let dylib = if let Some(lib) = lib.inner.upgrade() {
            lib
        } else {
            continue;
        };
        let info = dylib
            .user_data()
            .data()
            .iter()
            .next()
            .map(|data| data.downcast_ref::<RegisterInfo>().unwrap())
            .unwrap();
        let phdrs = if let Some(phdrs) = info.phdrs {
            phdrs
        } else {
            continue;
        };

        let mut info = dl_phdr_info {
            dlpi_addr: info.base as _,
            dlpi_name: info.name.as_ptr().cast(),
            dlpi_phdr: phdrs.as_ptr().cast(),
            dlpi_phnum: phdrs.len() as _,
            dlpi_adds: reader.len() as _,
            dlpi_subs: 0,
            dlpi_tls_modid: 0,
            dlpi_tls_data: core::ptr::null_mut(),
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
