use crate::{Dylib, OpenFlags};
use alloc::{borrow::ToOwned, boxed::Box, string::String, sync::Arc};
use core::marker::PhantomData;
use elf_loader::CoreComponent;
use indexmap::IndexMap;
use spin::{Lazy, RwLock};

impl Drop for Dylib<'_> {
    fn drop(&mut self) {
        if self.flags.contains(OpenFlags::RTLD_NODELETE) {
            return;
        } else if self.flags.contains(OpenFlags::CUSTOM_NOT_REGISTER) {
            log::debug!(
                "Call the fini function from the dylib [{}]",
                self.inner.shortname()
            );
            unsafe { self.inner.call_fini() };
            return;
        }
        let ref_count = self.inner.strong_count();
        // Dylib本身 + 全局
        let threshold =
            2 + self.deps.is_some() as usize + self.flags.contains(OpenFlags::RTLD_GLOBAL) as usize;
        if ref_count == threshold {
            log::info!("Destroying dylib [{}]", self.inner.shortname());
            log::debug!(
                "Call the fini function from the dylib [{}]",
                self.inner.shortname()
            );
            unsafe { self.inner.call_fini() };
            let mut lock = MANAGER.write();
            lock.all.shift_remove(self.inner.shortname());
            if self.flags.contains(OpenFlags::RTLD_GLOBAL) {
                lock.global.shift_remove(self.inner.shortname());
            }
            for dep in self.deps.as_ref().unwrap().iter().skip(1) {
                let dep_threshold = if let Some(lib) = lock.all.get(dep.shortname()) {
                    if lib.flags.contains(OpenFlags::RTLD_NODELETE) {
                        continue;
                    }
                    2 + lib.deps.is_some() as usize
                        + lib.flags.contains(OpenFlags::RTLD_GLOBAL) as usize
                } else {
                    continue;
                };
                if dep.strong_count() == dep_threshold {
                    log::info!("Destroying dylib [{}]", dep.shortname());
                    log::debug!(
                        "Call the fini function from the dylib [{}]",
                        dep.shortname()
                    );
                    unsafe { dep.call_fini() };
                    lock.all.shift_remove(dep.shortname());
                    lock.global.shift_remove(self.inner.shortname());
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct DylibState(u8);

impl Default for DylibState {
    fn default() -> Self {
        Self(0)
    }
}

impl DylibState {
    const USED_MASK: u8 = 0b10000000;
    const RELOCATED: u8 = 0b01111111;

	#[inline]
    pub(crate) fn set_unused(&mut self) -> &mut Self {
        self.0 &= !Self::USED_MASK;
        self
    }

	#[inline]
    pub(crate) fn set_used(&mut self) -> &mut Self {
        self.0 |= Self::USED_MASK;
        self
    }

	#[inline]
    pub(crate) fn is_used(&self) -> bool {
        self.0 & Self::USED_MASK != 0
    }

	#[inline]
    pub(crate) fn get_new_idx(&self) -> Option<u8> {
        let remove_used_bit = self.0 & !Self::USED_MASK;
        if remove_used_bit == Self::RELOCATED {
            None
        } else {
            Some(remove_used_bit)
        }
    }

	#[inline]
    pub(crate) fn set_relocated(&mut self) -> &mut Self {
        self.0 |= Self::RELOCATED;
        self
    }

	#[allow(unused)]
	#[inline]
    pub(crate) fn set_new_idx(&mut self, idx: u8) -> &mut Self {
        assert!(idx < Self::RELOCATED);
        self.0 |= idx;
        self
    }
}

#[derive(Clone)]
pub(crate) struct GlobalDylib {
    inner: CoreComponent,
    flags: OpenFlags,
    deps: Option<Arc<Box<[CoreComponent]>>>,
    pub(crate) state: DylibState,
}

unsafe impl Send for GlobalDylib {}
unsafe impl Sync for GlobalDylib {}

impl GlobalDylib {
    #[inline]
    pub(crate) fn get_dylib(&self) -> Dylib<'static> {
        debug_assert!(self.deps.is_some());
        Dylib {
            inner: self.inner.clone(),
            flags: self.flags,
            deps: self.deps.clone(),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub(crate) fn set_flags(&mut self, flags: OpenFlags) {
        self.flags = flags;
    }

    #[inline]
    pub(crate) fn flags(&self) -> OpenFlags {
        self.flags
    }

    #[inline]
    pub(crate) fn core_component(&self) -> CoreComponent {
        self.inner.clone()
    }

    #[inline]
    pub(crate) fn core_component_ref(&self) -> &CoreComponent {
        &self.inner
    }

    #[inline]
    pub(crate) fn deps(&self) -> Option<&Arc<Box<[CoreComponent]>>> {
        self.deps.as_ref()
    }
}

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

pub(crate) fn register(
    core: CoreComponent,
    flags: OpenFlags,
    deps: Option<Arc<Box<[CoreComponent]>>>,
    manager: &mut Manager,
    state: DylibState,
) {
    let shortname = core.shortname().to_owned();
    log::debug!(
        "Trying to register a library. Name: [{}] flags:[{:?}]",
        shortname,
        flags
    );
    manager.all.insert(
        shortname.to_owned(),
        GlobalDylib {
            state,
            inner: core.clone(),
            flags,
            deps,
        },
    );
    if flags.contains(OpenFlags::RTLD_GLOBAL) {
        manager.global.insert(shortname.to_owned(), core);
    }
}

#[cfg(feature = "std")]
pub(crate) fn global_find(name: &str) -> Option<*const ()> {
    log::debug!("Lazy Binding [{}]", name);
    crate::loader::builtin::BUILTIN
        .get(name)
        .copied()
        .or(MANAGER
            .read()
            .global
            .values()
            .find_map(|lib| unsafe { lib.get::<()>(name).map(|sym| sym.into_raw()) }))
}
