mod arch;
mod builtin;
pub mod dso;
#[cfg(feature = "ldso")]
mod ldso;
mod relocation;

use core::{
    ffi::CStr,
    fmt::Debug,
    marker::{self, PhantomData},
    ops,
};

use alloc::{format, sync::Arc};
use relocation::InternalLib;

use crate::{find_symbol_error, Result};
pub use dso::ELFLibrary;

#[derive(Debug)]
#[allow(unused)]
pub(crate) enum RelocatedLibraryInner {
    Internal(InternalLib),
    #[cfg(feature = "ldso")]
    External(ldso::ExternalLib),
}

#[derive(Clone, Debug)]
pub struct RelocatedLibrary {
    pub(crate) inner: Arc<RelocatedLibraryInner>,
}

unsafe impl Send for RelocatedLibrary {}
unsafe impl Sync for RelocatedLibrary {}

impl RelocatedLibrary {
    /// get the dynamic library name
    pub fn name(&self) -> &CStr {
        match self.inner.as_ref() {
            RelocatedLibraryInner::Internal(lib) => lib.name(),
            #[cfg(feature = "ldso")]
            RelocatedLibraryInner::External(lib) => lib.name(),
        }
    }

    #[cfg(feature = "std")]
    pub(crate) fn into_internal_lib(&self) -> Option<&InternalLib> {
        match self.inner.as_ref() {
            RelocatedLibraryInner::Internal(lib) => Some(lib),
            #[cfg(feature = "ldso")]
            RelocatedLibraryInner::External(_) => None,
        }
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
        match self.inner.as_ref() {
            RelocatedLibraryInner::Internal(lib) => lib.get_sym(name).map(|sym| Symbol {
                ptr: (lib.common_data().base() + sym.st_value as usize) as _,
                pd: PhantomData,
            }),
            #[cfg(feature = "ldso")]
            RelocatedLibraryInner::External(lib) => {
                let name = alloc::ffi::CString::new(name).unwrap();
                lib.get_sym(&name).map(|sym| Symbol {
                    ptr: sym as _,
                    pd: PhantomData,
                })
            }
        }
        .ok_or(find_symbol_error(format!("can not find symbol:{}", name)))
    }
}

impl Drop for RelocatedLibrary {
    fn drop(&mut self) {
        #[allow(irrefutable_let_patterns)]
        if let RelocatedLibraryInner::Internal(lib) = self.inner.as_ref() {
            if let Some(fini) = lib.common_data().fini_fn() {
                fini();
            }
            if let Some(fini_array) = lib.common_data().fini_array_fn() {
                for fini in *fini_array {
                    fini();
                }
            }
        }
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
