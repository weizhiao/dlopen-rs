use std::{ffi::OsStr, sync::Arc};

use elf::string_table::StringTable;

use crate::{
    file::ELFFile, loader::ELFLibraryInner, relocation::ELFRelocation, segment::ELFSegments,
    Result, Symbol,
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

    pub(crate) fn get_sym(&self, name: &str) -> Option<&Symbol> {
        self.inner.get_sym(name)
    }

    pub(crate) fn relocation(&self) -> &ELFRelocation {
        &self.inner.relocation
    }

    pub(crate) fn symtab(&self) -> *const Symbol {
        self.inner.symtab
    }

    pub(crate) fn strtab(&self) -> &StringTable {
        &self.inner.strtab
    }

    pub(crate) fn segments(&self) -> &ELFSegments {
        &self.inner.segments
    }

    pub(crate) fn tls(&self) -> &Option<Box<crate::tls::ELFTLS>> {
        &self.inner.tls
    }

    pub(crate) fn init_fn(&self) -> &Option<extern "C" fn()> {
        &self.inner.init_fn
    }

    pub(crate) fn init_array_fn(&self) -> &Option<&'static [extern "C" fn()]> {
        &self.inner.init_array_fn
    }

    pub(crate) fn fini_fn(&self) -> &Option<extern "C" fn()> {
        &self.inner.fini_fn
    }

    pub(crate) fn fini_array_fn(&self) -> &Option<&'static [extern "C" fn()]> {
        &self.inner.fini_array_fn
    }
}

#[derive(Debug, Clone)]
pub struct ELFHandle {
    pub(crate) inner: ELFLibrary,
    inner_libs: Arc<Vec<Arc<ELFLibraryInner>>>,
}

impl ELFHandle {
    pub(crate) fn new(
        lib: ELFLibrary,
        inner_libs: Vec<Arc<ELFLibraryInner>>,
    ) -> ELFHandle {
        ELFHandle {
            inner: lib,
            inner_libs: Arc::new(inner_libs),
        }
    }

    pub fn get_sym(&self, name: &str) -> Option<*const ()> {
        self.inner.get_sym(name).map(|sym| unsafe {
            self.inner
                .segments()
                .as_mut_ptr()
                .add(sym.st_value as usize) as *const ()
        })
    }
}

impl Drop for ELFHandle {
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
