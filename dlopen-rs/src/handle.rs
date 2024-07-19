use std::{
    ffi::OsStr,
    marker::{self, PhantomData},
    ops,
    sync::Arc,
};

use elf::string_table::StringTable;

use crate::{
    file::ELFFile, loader::ELFLibraryInner, relocation::ELFRelocation, segment::ELFSegments,
    ELFSymbol, Result,
};

#[derive(Debug, Clone)]
pub struct ELFLibrary {
    pub(crate) inner: Arc<ELFLibraryInner>,
}

impl ELFLibrary {
    pub fn from_file<P: AsRef<OsStr>>(path: P) -> Result<ELFLibrary> {
        let file = ELFFile::from_file(path.as_ref())?;
        let inner = ELFLibraryInner::load_library(file)?;
        Ok(ELFLibrary {
            inner: Arc::new(inner),
        })
    }

    pub fn from_binary(bytes: &[u8]) -> Result<ELFLibrary> {
        let file = ELFFile::from_binary(bytes);
        let inner = ELFLibraryInner::load_library(file)?;
        Ok(ELFLibrary {
            inner: Arc::new(inner),
        })
    }

    #[inline]
    pub(crate) fn get_sym(&self, name: &str) -> Option<&ELFSymbol> {
        self.inner.get_sym(name)
    }

    #[inline]
    pub(crate) fn relocation(&self) -> &ELFRelocation {
        &self.inner.relocation
    }

    #[inline]
    pub(crate) fn symtab(&self) -> *const ELFSymbol {
        self.inner.symtab
    }

    #[inline]
    pub(crate) fn strtab(&self) -> &StringTable {
        &self.inner.strtab
    }

    #[inline]
    pub(crate) fn segments(&self) -> &ELFSegments {
        &self.inner.segments
    }

    #[inline]
    pub(crate) fn tls(&self) -> &Option<Box<crate::tls::ELFTLS>> {
        &self.inner.tls
    }

    #[inline]
    pub(crate) fn init_fn(&self) -> &Option<extern "C" fn()> {
        &self.inner.init_fn
    }

    #[inline]
    pub(crate) fn init_array_fn(&self) -> &Option<&'static [extern "C" fn()]> {
        &self.inner.init_array_fn
    }

    #[inline]
    pub(crate) fn fini_fn(&self) -> &Option<extern "C" fn()> {
        &self.inner.fini_fn
    }

    #[inline]
    pub(crate) fn fini_array_fn(&self) -> &Option<&'static [extern "C" fn()]> {
        &self.inner.fini_array_fn
    }
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct ELFInstance {
    pub(crate) inner: ELFLibrary,
    needed_libs: Arc<Vec<ELFInstance>>,
}

impl ELFInstance {
    pub(crate) fn new(lib: ELFLibrary, needed_libs: Vec<ELFInstance>) -> ELFInstance {
        ELFInstance {
            inner: lib,
            needed_libs: Arc::new(needed_libs),
        }
    }

    pub(crate) fn get_sym(&self, name: &str) -> Option<*const ()> {
        self.inner.get_sym(name).map(|sym| unsafe {
            self.inner
                .segments()
                .as_mut_ptr()
                .add(sym.st_value as usize) as *const ()
        })
    }

    pub fn get<'lib, T>(&'lib self, name: &str) -> Option<Symbol<'lib, T>> {
        self.get_sym(name).map(|sym| Symbol {
            ptr: sym as _,
            pd: PhantomData,
        })
    }
}

impl Drop for ELFInstance {
    fn drop(&mut self) {
        if let Some(fini) = self.inner.fini_fn() {
            fini();
        }
        if let Some(fini_array) = self.inner.fini_array_fn() {
            for fini in *fini_array {
                fini();
            }
        }
    }
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
