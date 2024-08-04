use core::{
    fmt::Debug,
    marker::{self, PhantomData},
    ops,
};

use alloc::{boxed::Box, format, sync::Arc, vec::Vec};
use elf::string_table::StringTable;

use crate::{
    file::ELFFile, find_symbol_error, hashtable::ELFHashTable, relocation::ELFRelocation,
    segment::ELFSegments, unwind::ELFUnwind, ELFSymbol, Result,
};

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct ELFLibraryInner {
    pub(crate) common: CommonInner,
    /// rela.dyn and rela.plt
    pub(crate) relocation: ELFRelocation,
    /// .init
    pub(crate) init_fn: Option<extern "C" fn()>,
    /// .init_array
    pub(crate) init_array_fn: Option<&'static [extern "C" fn()]>,
    /// needed libs' name
    pub(crate) needed_libs: Vec<&'static str>,
}

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct CommonInner {
    /// .gnu.hash
    pub(crate) hashtab: ELFHashTable,
    /// .dynsym
    pub(crate) symtab: *const ELFSymbol,
    /// .dynstr
    pub(crate) strtab: elf::string_table::StringTable<'static>,
    /// .eh_frame
    pub(crate) unwind: Option<ELFUnwind>,
    /// semgents
    pub(crate) segments: ELFSegments,
    /// .fini
    pub(crate) fini_fn: Option<extern "C" fn()>,
    /// .fini_array
    pub(crate) fini_array_fn: Option<&'static [extern "C" fn()]>,
    /// .tbss and .tdata
    #[cfg(feature = "tls")]
    pub(crate) tls: Option<Box<crate::tls::ELFTLS>>,
}

impl CommonInner {
    pub(crate) fn get_sym(&self, name: &str) -> Option<&ELFSymbol> {
        let bytes = name.as_bytes();
        let name = if *bytes.last().unwrap() == 0 {
            &bytes[..bytes.len() - 1]
        } else {
            bytes
        };
        let symbol = unsafe { self.hashtab.find(name, self.symtab, &self.strtab) };
        symbol
    }
}

#[derive(Debug)]
pub struct ELFLibrary {
    pub(crate) inner: ELFLibraryInner,
}

impl ELFLibrary {
    /// Find and load a elf dynamic library from path.
    ///
    /// The `filename` argument may be either:
    ///
    /// * A library filename;
    /// * The absolute path to the library;
    /// * A relative (to the current working directory) path to the library.
    /// # Examples
    ///
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let lib = ELFLibrary::from_file("/path/to/awesome.module")
    ///		.unwrap();
    /// ```
    ///
    #[cfg(feature = "std")]
    pub fn from_file<P: AsRef<std::ffi::OsStr>>(path: P) -> Result<ELFLibrary> {
        let file = ELFFile::from_file(path.as_ref())?;
        let inner = ELFLibraryInner::load_library(file)?;
        Ok(ELFLibrary { inner })
    }

    /// load a elf dynamic library from bytes
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let path = Path::new("/path/to/awesome.module");
    /// let bytes = std::fs::read(path).unwrap();
    /// let lib = ELFLibrary::from_binary(&bytes).unwarp();
    /// ```
    pub fn from_binary(bytes: &[u8]) -> Result<ELFLibrary> {
        let file = ELFFile::from_binary(bytes);
        let inner = ELFLibraryInner::load_library(file)?;
        Ok(ELFLibrary { inner })
    }

    pub fn needed_libs(&self) -> &Vec<&str> {
        &self.inner.needed_libs
    }

    #[inline]
    pub(crate) fn relocation(&self) -> &ELFRelocation {
        &self.inner.relocation
    }

    #[inline]
    pub(crate) fn symtab(&self) -> *const ELFSymbol {
        self.inner.common.symtab
    }

    #[inline]
    pub(crate) fn strtab(&self) -> &StringTable {
        &self.inner.common.strtab
    }

    #[inline]
    pub(crate) fn segments(&self) -> &ELFSegments {
        &self.inner.common.segments
    }

    #[inline]
    #[cfg(feature = "tls")]
    pub(crate) fn tls(&self) -> *const crate::tls::ELFTLS {
        self.inner.common.tls.as_ref().unwrap().as_ref() as *const crate::tls::ELFTLS
    }

    #[inline]
    pub(crate) fn init_fn(&self) -> &Option<extern "C" fn()> {
        &self.inner.init_fn
    }

    #[inline]
    pub(crate) fn init_array_fn(&self) -> &Option<&'static [extern "C" fn()]> {
        &self.inner.init_array_fn
    }
}

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct RelocatedLibraryInner {
    pub(crate) common: CommonInner,
    pub(crate) internal_libs: Vec<RelocatedLibrary>,
    pub(crate) external_libs: Option<Vec<Box<dyn ExternLibrary>>>,
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
            internal_libs,
            external_libs,
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

#[derive(Debug)]
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
