mod arch;
mod builtin;
#[cfg(feature = "debug")]
pub(crate) mod debug;
pub mod dso;
#[cfg(feature = "ldso")]
mod ldso;
mod relocation;

use core::{
    ffi::CStr,
    fmt::Debug,
    marker::{self, PhantomData},
    ops,
    sync::atomic::{AtomicBool, Ordering},
};

use alloc::{ffi::CString, format, sync::Arc};
use arch::ELFSymbol;
use dso::{version::SymbolRequireVersion, ExtraData, SymbolData};

use crate::{find_symbol_error, Result};
pub use dso::ELFLibrary;

#[allow(unused)]
enum LibraryExtraData {
    Internal(ExtraData),
    #[cfg(feature = "ldso")]
    External(ldso::ExtraData),
}

impl Drop for LibraryExtraData {
    fn drop(&mut self) {
        if let LibraryExtraData::Internal(extra) = self {
            if let Some(fini) = extra.fini_fn() {
                fini();
            }
            if let Some(fini_array) = extra.fini_array_fn() {
                for fini in fini_array {
                    fini();
                }
            }
        }
    }
}

#[allow(unused)]
pub(crate) struct RelocatedLibraryInner {
    name: CString,
    base: usize,
    symbols: SymbolData,
    tls: Option<usize>,
    extra: LibraryExtraData,
}

impl Debug for RelocatedLibraryInner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RelocatedLibraryInner").finish()
    }
}

impl RelocatedLibraryInner {
    pub(crate) fn get_sym(
        &self,
        name: &str,
        version: &Option<SymbolRequireVersion>,
    ) -> Option<(&ELFSymbol, usize)> {
        self.symbols.get_sym(name, version)
    }

    pub(crate) fn tls(&self) -> Option<usize> {
        self.tls
    }
}

#[derive(Clone, Debug)]
pub struct RelocatedLibrary {
    pub(crate) inner: Arc<(AtomicBool, RelocatedLibraryInner)>,
}

unsafe impl Send for RelocatedLibrary {}
unsafe impl Sync for RelocatedLibrary {}

impl RelocatedLibrary {
    /// get dependence libraries
    pub fn dep_libs(&self) -> Option<&Vec<RelocatedLibrary>> {
        match &self.inner().extra {
            LibraryExtraData::Internal(extra_data) => extra_data.get_dep_libs(),
            LibraryExtraData::External(_) => None,
        }
    }

    /// get the dynamic library name
    pub fn name(&self) -> &CStr {
        &self.inner().name
    }

    #[allow(unused)]
    pub(crate) fn is_register(&self) -> bool {
        self.inner.0.load(Ordering::Relaxed)
    }

    #[allow(unused)]
    pub(crate) fn set_register(&self) {
        self.inner.0.store(true, Ordering::Relaxed);
    }

    #[cfg(feature = "std")]
    pub(crate) fn get_extra_data(&self) -> Option<&ExtraData> {
        match &self.inner().extra {
            LibraryExtraData::Internal(lib) => Some(lib),
            #[cfg(feature = "ldso")]
            LibraryExtraData::External(_) => None,
        }
    }

    pub(crate) fn inner(&self) -> &RelocatedLibraryInner {
        &self.inner.1
    }

    pub(crate) fn base(&self) -> usize {
        self.inner().base
    }

    /// Get a pointer to a function or static variable by symbol name.
    ///
    /// The symbol is interpreted as-is; no mangling is done. This means that symbols like `x::y` are
    /// most likely invalid.
    ///
    /// # Safety
    ///
    /// Users of this API must specify the correct type of the function or variable loaded.
    ///
    ///
    /// # Examples
    ///
    /// Given a loaded library:
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let lib = ELFLibrary::from_file("/path/to/awesome.module")
    ///		.unwrap()
    ///		.relocate(&[])
    ///		.unwrap();
    /// ```
    ///
    /// Loading and using a function looks like this:
    ///
    /// ```no_run
    /// unsafe {
    ///     let awesome_function: Symbol<unsafe extern fn(f64) -> f64> =
    ///         lib.get("awesome_function").unwrap();
    ///     awesome_function(0.42);
    /// }
    /// ```
    ///
    /// A static variable may also be loaded and inspected:
    ///
    /// ```no_run
    /// unsafe {
    ///     let awesome_variable: Symbol<*mut f64> = lib.get("awesome_variable").unwrap();
    ///     **awesome_variable = 42.0;
    /// };
    /// ```
    pub unsafe fn get<'lib, T>(&'lib self, name: &str) -> Result<Symbol<'lib, T>> {
        self.inner
            .as_ref()
            .1
            .get_sym(name, &None)
            .map(|(sym, _)| Symbol {
                ptr: (self.base() + sym.st_value as usize) as _,
                pd: PhantomData,
            })
            .ok_or(find_symbol_error(format!("can not find symbol:{}", name)))
    }
}

pub trait ExternLibrary {
    /// Get the symbol of the dynamic library, and the return value is the address of the symbol
    /// # Examples
    ///
    /// ```
    /// #[derive(Debug, Clone)]
    /// struct MyLib(Arc<Library>);
    ///
    /// impl ExternLibrary for MyLib {
    /// 	fn get_sym(&self, name: &str) -> Option<*const ()> {
    /// 		let sym: Option<*const ()> = unsafe {
    ///  			self.0.get::<*const usize>(name.as_bytes())
    ///				.map_or(None, |sym| Some(sym.into_raw().into_raw() as _))
    ///			};
    ///			sym
    /// 	}
    ///}
    /// ```
    fn get_sym(&self, name: &str) -> Option<*const ()>;
}

#[derive(Debug, Clone)]
pub struct Symbol<'lib, T: 'lib> {
    ptr: *mut (),
    pd: marker::PhantomData<&'lib T>,
}

impl<'lib, T> ops::Deref for Symbol<'lib, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*(&self.ptr as *const *mut _ as *const T) }
    }
}
