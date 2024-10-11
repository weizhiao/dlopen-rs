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

use alloc::{ffi::CString, format, sync::Arc, vec::Vec};
use dso::{
    symbol::{SymbolData, SymbolInfo},
    ExtraData,
};

use crate::{find_symbol_error, Result};
pub use dso::ELFLibrary;

#[allow(unused)]
#[derive(Debug)]
enum LibraryExtraData {
    Internal(ExtraData),
    #[cfg(feature = "ldso")]
    External(ldso::ExtraData),
}

impl Drop for LibraryExtraData {
    fn drop(&mut self) {
        #[allow(irrefutable_let_patterns)]
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
    #[cfg(feature = "tls")]
    tls: Option<usize>,
    #[cfg(feature = "debug")]
    link_map: debug::DebugInfo,
    extra: LibraryExtraData,
}

impl Debug for RelocatedLibraryInner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RelocatedLibrary")
            .field("name", &self.name)
            .field("base", &self.base)
            .field("extra", &self.extra)
            .finish()
    }
}

impl RelocatedLibraryInner {
    #[inline]
    pub(crate) fn symbols(&self) -> &SymbolData {
        &self.symbols
    }

    #[cfg(feature = "tls")]
    pub(crate) fn tls(&self) -> Option<usize> {
        self.tls
    }
}

#[derive(Clone)]
pub struct RelocatedLibrary {
    pub(crate) inner: Arc<(AtomicBool, RelocatedLibraryInner)>,
}

impl Debug for RelocatedLibrary {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner().fmt(f)
    }
}

unsafe impl Send for RelocatedLibrary {}
unsafe impl Sync for RelocatedLibrary {}

impl RelocatedLibrary {
    /// Retrieves the list of dependent libraries.
    ///
    /// This method returns an optional reference to a vector of `RelocatedLibrary` instances,
    /// which represent the libraries that the current dynamic library depends on.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// if let Some(dependencies) = library.dep_libs() {
    ///     for lib in dependencies {
    ///         println!("Dependency: {:?}", lib);
    ///     }
    /// } else {
    ///     println!("No dependencies found.");
    /// }
    /// ```
    pub fn dep_libs(&self) -> Option<&Vec<RelocatedLibrary>> {
        match &self.inner().extra {
            LibraryExtraData::Internal(extra_data) => extra_data.get_dep_libs(),
            #[cfg(feature = "ldso")]
            LibraryExtraData::External(_) => None,
        }
    }

    /// Retrieves the name of the dynamic library.
    ///
    /// This method returns a string slice that represents the name of the dynamic library.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// let library_name = library.name();
    /// println!("The dynamic library name is: {}", library_name);
    /// ```
    pub fn name(&self) -> &str {
        self.inner().name.to_str().unwrap()
    }

    pub fn cname(&self) -> &CStr {
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

    pub fn base(&self) -> usize {
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
        self.inner()
            .symbols()
            .get_sym(&SymbolInfo::new(name))
            .map(|sym| Symbol {
                ptr: (self.base() + sym.st_value as usize) as _,
                pd: PhantomData,
            })
            .ok_or(find_symbol_error(format!("can not find symbol:{}", name)))
    }

    /// Attempts to load a versioned symbol from the dynamically-linked library.
    ///
    /// # Safety
    /// This function is unsafe because it involves raw pointer manipulation and
    /// dereferencing. The caller must ensure that the library handle is valid
    /// and that the symbol exists and has the correct type.
    ///
    /// # Parameters
    /// - `&'lib self`: A reference to the library instance from which the symbol will be loaded.
    /// - `name`: The name of the symbol to load.
    /// - `version`: The version of the symbol to load.
    ///
    /// # Returns
    /// If the symbol is found and has the correct type, this function returns
    /// `Ok(Symbol<'lib, T>)`, where `Symbol` is a wrapper around a raw function pointer.
    /// If the symbol cannot be found or an error occurs, it returns an `Err` with a message.
    ///
    /// # Examples
    /// ```
    /// let symbol = unsafe { lib.get_version::<fn()>>("function_name", "1.0").unwrap() };
    /// ```
    ///
    /// # Errors
    /// Returns a custom error if the symbol cannot be found, or if there is a problem
    /// retrieving the symbol.
    #[cfg(feature = "version")]
    pub unsafe fn get_version<'lib, T>(
        &'lib self,
        name: &str,
        version: &str,
    ) -> Result<Symbol<'lib, T>> {
        let version = dso::version::SymbolVersion::new(version);
        self.inner()
            .symbols()
            .get_sym(&SymbolInfo::new_with_version(name, version))
            .map(|sym| Symbol {
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
