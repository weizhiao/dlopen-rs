use crate::{Dylib, OpenFlags};
use alloc::{borrow::ToOwned, string::String};
use elf_loader::CoreComponent;
use indexmap::IndexMap;
use spin::{Lazy, RwLock};

pub(crate) const IS_RELOCATED: u8 = u8::MAX;

impl Drop for Dylib<'_> {
    fn drop(&mut self) {
        if self.flags.contains(OpenFlags::RTLD_NODELETE) {
            return;
        } else if self.flags.contains(OpenFlags::CUSTOM_NOT_REGISTER) {
            unsafe { self.inner.call_fini() };
            return;
        }
        let ref_count = self.inner.count();
        // Dylib本身 + 全局
        let threshold =
            2 + self.flags.contains(OpenFlags::RTLD_GLOBAL) as usize + self.deps.is_some() as usize;
        if ref_count == threshold {
            let mut lock = MANAGER.write();
            if self.flags.contains(OpenFlags::RTLD_GLOBAL) {
                lock.global.shift_remove(self.inner.shortname());
            }
            // 在RTLD_LOCAL的情况下这段代码会执行两次，不过影响不大
            let drop1 = if let Some(lib) = lock.all.shift_remove(self.inner.shortname()) {
                log::info!("Destroying dylib [{}]", self.inner.shortname());
                if lib.new_idx == IS_RELOCATED {
                    log::debug!(
                        "Call the fini function from the dylib [{}]",
                        self.inner.shortname()
                    );
                    unsafe { self.inner.call_fini() };
                }
                Some(lib)
            } else {
                None
            };
            // 防止死锁
            drop(lock);
            drop(drop1);
        }
    }
}

#[derive(Clone)]
pub(crate) struct GlobalDylib {
    pub(crate) dylib: Dylib<'static>,
    pub(crate) new_idx: u8,
	#[allow(unused)]
    pub(crate) is_mark: bool,
}

unsafe impl Send for GlobalDylib {}
unsafe impl Sync for GlobalDylib {}

pub(crate) struct Manager {
    pub(crate) all: IndexMap<String, GlobalDylib>,
    pub(crate) global: IndexMap<String, CoreComponent>,
}

pub(crate) static MANAGER: Lazy<RwLock<Manager>> = Lazy::new(|| {
    RwLock::new(Manager {
        all: IndexMap::new(),
        global: IndexMap::new(),
    })
});

pub(crate) fn register(lib: Dylib<'static>, manager: &mut Manager, is_mark: bool, new_idx: Option<u8>) {
    let shortname = lib.inner.shortname().to_owned();
    log::debug!(
        "Trying to register a library. Name: [{}] flags:[{:?}]",
        shortname,
        lib.flags
    );
    let core = lib.inner.clone();
    let flags = lib.flags;
    manager.all.insert(
        shortname.to_owned(),
        GlobalDylib {
            dylib: lib,
            new_idx: new_idx.unwrap_or(u8::MAX),
            is_mark,
        },
    );
    if flags.contains(OpenFlags::RTLD_GLOBAL) {
        manager.global.insert(shortname.to_owned(), core);
    }
}

#[cfg(feature = "std")]
pub(crate) fn global_find(name: &str) -> Option<*const ()> {
    log::debug!("Lazy Binding [{}]", name);
    crate::loader::builtin::BUILTIN.get(name).copied().or(MANAGER
        .read()
        .global
        .values()
        .find_map(|lib| unsafe { lib.get::<()>(name).map(|sym| sym.into_raw()).ok() }))
}

#[cfg(feature = "std")]
pub unsafe extern "C" fn dl_iterate_phdr_impl(
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

    use crate::init::OLD_DL_ITERATE_PHDR;
    let reader = MANAGER.read();
    let mut ret = OLD_DL_ITERATE_PHDR.unwrap()(callback, data);
    if ret != 0 {
        return ret;
    }
    for lib in reader.all.values() {
        let phdrs = lib.dylib.phdrs();
        if phdrs.is_empty() {
            continue;
        }
        let mut info = dl_phdr_info {
            dlpi_addr: lib.dylib.base() as _,
            dlpi_name: lib.dylib.cname().as_ptr(),
            dlpi_phdr: phdrs.as_ptr().cast(),
            dlpi_phnum: phdrs.len() as _,
            dlpi_adds: reader.all.len() as _,
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
