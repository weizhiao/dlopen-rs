mod arch;
mod builtin;
pub mod dso;
mod relocation;

use core::{
    fmt::Debug,
    marker::{self, PhantomData},
    ops,
};

use alloc::{boxed::Box, format, sync::Arc, vec::Vec};

use crate::{find_symbol_error, Result};
use dso::CommonElfData;
pub use dso::ELFLibrary;

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct RelocatedLibraryInner {
    pub(crate) common: CommonElfData,
    pub(crate) internal_libs: Box<[RelocatedLibrary]>,
    pub(crate) external_libs: Option<Box<[Box<dyn ExternLibrary>]>>,
}

#[derive(Debug, Clone)]
pub struct RelocatedLibrary {
    pub(crate) inner: Arc<RelocatedLibraryInner>,
}

impl RelocatedLibrary {
    pub(crate) fn new(
        lib: ELFLibrary,
        internal_libs: Vec<RelocatedLibrary>,
        external_libs: Option<Vec<Box<dyn ExternLibrary>>>,
    ) -> RelocatedLibrary {
        let inner = RelocatedLibraryInner {
            common: lib.inner.common,
            internal_libs: internal_libs.into_boxed_slice(),
            external_libs: external_libs.map(|libs| libs.into_boxed_slice()),
        };

        RelocatedLibrary {
            inner: Arc::new(inner),
        }
    }

    pub(crate) fn get_sym(&self, name: &str) -> Option<*const ()> {
        self.inner.common.get_sym(name).map(|sym| unsafe {
            self.inner
                .common
                .segments
                .as_mut_ptr()
                .add(sym.st_value as usize) as *const ()
        })
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
        self.get_sym(name)
            .map(|sym| Symbol {
                ptr: sym as _,
                pd: PhantomData,
            })
            .ok_or(find_symbol_error(format!("can not find symbol:{}", name)))
    }
}

impl Drop for RelocatedLibrary {
    fn drop(&mut self) {
        if let Some(fini) = self.inner.common.fini_fn {
            fini();
        }
        if let Some(fini_array) = self.inner.common.fini_array_fn {
            for fini in fini_array {
                fini();
            }
        }
    }
}

pub trait ExternLibrary: Debug {
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
