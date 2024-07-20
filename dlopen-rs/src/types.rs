use std::{
    ffi::OsStr,
    marker::{self, PhantomData},
    ops,
    sync::Arc,
};

use elf::string_table::StringTable;

use crate::{
    file::ELFFile, hash::ELFHashTable, relocation::ELFRelocation, segment::ELFSegments,
    unwind::ELFUnwind, ELFSymbol, Result,
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
    pub fn from_file<P: AsRef<OsStr>>(path: P) -> Result<ELFLibrary> {
        let file = ELFFile::from_file(path.as_ref())?;
        let inner = ELFLibraryInner::load_library(file)?;
        Ok(ELFLibrary { inner })
    }

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
    pub(crate) fn tls(&self) -> &Option<Box<crate::tls::ELFTLS>> {
        &self.inner.common.tls
    }

    #[inline]
    pub(crate) fn init_fn(&self) -> &Option<extern "C" fn()> {
        &self.inner.init_fn
    }

    #[inline]
    pub(crate) fn init_array_fn(&self) -> &Option<&'static [extern "C" fn()]> {
        &self.inner.init_array_fn
    }

    pub(crate) fn unwind(&self) -> &Option<ELFUnwind> {
        &self.inner.common.unwind
    }
}

#[derive(Debug)]
#[allow(unused)]
struct RelocatedLibraryInner {
    common: CommonInner,
    needed_libs: Vec<RelocatedLibrary>,
}

#[derive(Debug, Clone)]
pub struct RelocatedLibrary {
    inner: Arc<RelocatedLibraryInner>,
}

impl RelocatedLibrary {
    pub(crate) fn new(lib: ELFLibrary, needed_libs: Vec<RelocatedLibrary>) -> RelocatedLibrary {
        let inner = RelocatedLibraryInner {
            common: lib.inner.common,
            needed_libs,
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

    pub fn get<'lib, T>(&'lib self, name: &str) -> Option<Symbol<'lib, T>> {
        self.get_sym(name).map(|sym| Symbol {
            ptr: sym as _,
            pd: PhantomData,
        })
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
