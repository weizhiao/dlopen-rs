mod binary;
pub(crate) mod dynamic;
mod ehdr;
mod ehframe;
#[cfg(feature = "std")]
mod file;
mod hashtab;
mod segment;
#[cfg(feature = "tls")]
pub(crate) mod tls;
#[cfg(feature = "version")]
pub(crate) mod version;

use super::{
    arch::{ELFSymbol, Phdr, Rela, PHDR_SIZE},
    LibraryExtraData, RelocatedLibrary, RelocatedLibraryInner,
};
use crate::{parse_dynamic_error, Result};
use alloc::ffi::CString;
use alloc::{boxed::Box, vec::Vec};
use binary::ELFBinary;
use core::{
    fmt::Debug,
    ops::{Range, Shr},
};
use dynamic::ELFRawDynamic;
use ehframe::EhFrame;
use elf::abi::*;
use hashtab::ELFGnuHash;
use segment::{ELFRelro, ELFSegments, MASK, PAGE_SIZE};

#[derive(Clone)]
pub(crate) struct ELFStringTable<'data> {
    data: &'data [u8],
}

impl<'data> ELFStringTable<'data> {
    fn new(data: &'data [u8]) -> Self {
        ELFStringTable { data }
    }

    fn get(&self, offset: usize) -> &'data str {
        let start = self.data.get(offset..).unwrap();
        let end = start.iter().position(|&b| b == 0u8).unwrap();
        unsafe { core::str::from_utf8_unchecked(start.split_at(end).0) }
    }
}

pub(crate) struct ELFRelocation {
    pub(crate) pltrel: Option<&'static [Rela]>,
    pub(crate) rel: Option<&'static [Rela]>,
}

pub(crate) struct SymbolData {
    /// .gnu.hash
    pub hashtab: ELFGnuHash,
    /// .dynsym
    pub symtab: *const ELFSymbol,
    /// .dynstr
    pub strtab: ELFStringTable<'static>,
    #[cfg(feature = "version")]
    /// .gnu.version
    pub version: Option<version::ELFVersion>,
}

pub(crate) struct SymbolInfo<'a> {
    pub name: &'a str,
    #[cfg(feature = "version")]
    version: Option<version::SymbolVersion<'a>>,
}

impl<'a> SymbolInfo<'a> {
    pub(crate) const fn new(name: &'a str) -> Self {
        SymbolInfo {
            name,
            #[cfg(feature = "version")]
            version: None,
        }
    }

    #[cfg(feature = "version")]
    pub(crate) const fn new_with_version(
        name: &'a str,
        version: version::SymbolVersion<'a>,
    ) -> Self {
        SymbolInfo {
            name,
            version: Some(version),
        }
    }
}

impl SymbolData {
    pub(crate) fn get_sym(&self, symbol: &SymbolInfo) -> Option<&ELFSymbol> {
        let hash = ELFGnuHash::gnu_hash(symbol.name.as_bytes());
        let bloom_width: u32 = 8 * size_of::<usize>() as u32;
        let bloom_idx = (hash / (bloom_width)) as usize % self.hashtab.blooms.len();
        let filter = self.hashtab.blooms[bloom_idx] as u64;
        if filter & (1 << (hash % bloom_width)) == 0 {
            return None;
        }
        let hash2 = hash.shr(self.hashtab.nshift);
        if filter & (1 << (hash2 % bloom_width)) == 0 {
            return None;
        }
        let table_start_idx = self.hashtab.table_start_idx as usize;
        let chain_start_idx = unsafe {
            self.hashtab
                .buckets
                .add((hash as usize) % self.hashtab.nbucket as usize)
                .read()
        } as usize;
        if chain_start_idx == 0 {
            return None;
        }
        let mut dynsym_idx = chain_start_idx;
        let mut cur_chain = unsafe { self.hashtab.chains.add(dynsym_idx - table_start_idx) };
        let mut cur_symbol_ptr = unsafe { self.symtab.add(dynsym_idx) };
        loop {
            let chain_hash = unsafe { cur_chain.read() };
            if hash | 1 == chain_hash | 1 {
                let cur_symbol = unsafe { &*cur_symbol_ptr };
                let sym_name = self.strtab.get(cur_symbol.st_name as usize);
                #[cfg(feature = "version")]
                if sym_name == symbol.name && self.check_match(dynsym_idx, &symbol.version) {
                    return Some(cur_symbol);
                }
                #[cfg(not(feature = "version"))]
                if sym_name == symbol.name {
                    return Some(cur_symbol);
                }
            }
            if chain_hash & 1 != 0 {
                break;
            }
            cur_chain = unsafe { cur_chain.add(1) };
            cur_symbol_ptr = unsafe { cur_symbol_ptr.add(1) };
            dynsym_idx += 1;
        }
        None
    }

    pub(crate) fn rel_symbol(&self, idx: usize) -> (&ELFSymbol, SymbolInfo) {
        let symbol = unsafe { &*self.symtab.add(idx) };
        let name = self.strtab.get(symbol.st_name as usize);
        (
            symbol,
            SymbolInfo {
                name,
                #[cfg(feature = "version")]
                version: self.get_requirement(idx),
            },
        )
    }
}

#[allow(unused)]
pub(crate) struct ExtraData {
    /// phdrs
    #[cfg(feature = "std")]
    phdrs: Option<&'static [Phdr]>,
    /// .eh_frame
    unwind: Option<EhFrame>,
    /// semgents
    segments: ELFSegments,
    /// .fini
    fini_fn: Option<extern "C" fn()>,
    /// .fini_array
    fini_array_fn: Option<&'static [extern "C" fn()]>,
    /// .tbss and .tdata
    #[cfg(feature = "tls")]
    tls: Option<Box<tls::ELFTLS>>,
    /// user data
    user_data: Option<Box<dyn Fn(&str) -> Option<*const ()>>>,
    /// dependency libraries
    dep_libs: Option<Vec<RelocatedLibrary>>,
}

impl Debug for ExtraData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut binding = f.debug_struct("ExtraData");
        let f = binding.field("segments", &self.segments);
        if let Some(dep_libs) = &self.dep_libs {
            f.field("dep_libs", dep_libs);
        }
        f.finish()
    }
}

impl ExtraData {
    #[cfg(feature = "std")]
    pub(crate) fn phdrs(&self) -> Option<&[Phdr]> {
        self.phdrs
    }

    #[inline]
    pub(crate) fn base(&self) -> usize {
        self.segments.base()
    }

    #[inline]
    #[cfg(feature = "tls")]
    pub(crate) fn tls(&self) -> Option<*const tls::ELFTLS> {
        self.tls.as_ref().map(|val| val.as_ref() as _)
    }

    #[inline]
    pub(crate) fn set_user_data(
        &mut self,
        user_data: Option<Box<dyn Fn(&str) -> Option<*const ()>>>,
    ) {
        self.user_data = user_data;
    }

    #[inline]
    pub(crate) fn set_dep_libs(&mut self, dep_libs: Vec<RelocatedLibrary>) {
        self.dep_libs = Some(dep_libs);
    }

    #[inline]
    pub(crate) fn get_dep_libs(&self) -> Option<&Vec<RelocatedLibrary>> {
        self.dep_libs.as_ref()
    }

    pub(crate) fn fini_fn(&self) -> Option<extern "C" fn()> {
        self.fini_fn
    }

    pub(crate) fn fini_array_fn(&self) -> Option<&[extern "C" fn()]> {
        self.fini_array_fn
    }
}

#[allow(unused)]
pub(crate) struct ELFLibraryInner {
    /// file name
    name: CString,
    /// elf symbols
    symbols: SymbolData,
    /// debug link map
    #[cfg(feature = "debug")]
    link_map: super::debug::DebugInfo,
    /// extra elf data
    extra: ExtraData,
    /// rela.dyn and rela.plt
    relocation: ELFRelocation,
    /// GNU_RELRO segment
    relro: Option<ELFRelro>,
    /// .init
    init_fn: Option<extern "C" fn()>,
    /// .init_array
    init_array_fn: Option<&'static [extern "C" fn()]>,
    /// needed libs' name
    needed_libs: Vec<&'static str>,
}

impl Debug for ELFLibraryInner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ELFLibrary")
            .field("name", &self.name)
            .field("extra", &self.extra)
            .field("needed_libs", &self.needed_libs)
            .finish()
    }
}

pub struct ELFLibrary {
    inner: ELFLibraryInner,
}

impl Debug for ELFLibrary {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner.fmt(f)
    }
}

impl From<ELFLibrary> for RelocatedLibraryInner {
    fn from(value: ELFLibrary) -> Self {
        RelocatedLibraryInner {
            #[cfg(feature = "debug")]
            link_map: value.inner.link_map,
            name: value.inner.name,
            base: value.inner.extra.base(),
            symbols: value.inner.symbols,
            #[cfg(feature = "tls")]
            tls: value.inner.extra.tls().map(|tls| tls as usize),
            extra: LibraryExtraData::Internal(value.inner.extra),
        }
    }
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
        let file_name = path.as_ref().to_str().unwrap();
        let file = std::fs::File::open(path.as_ref())?;
        let mut file = super::dso::file::ELFFile::new(file);
        let inner = file.load(CString::new(file_name).unwrap())?;
        Ok(ELFLibrary { inner })
    }

    /// Creates a new `ELFLibrary` instance from an open file handle.
    ///
    /// # Features
    /// This function is only available when the "std" feature is enabled. The "std"
    /// feature must be specified in the dependency section of the Cargo.toml file.
    ///
    /// # Parameters
    /// - `file`: A open file handle (`std::fs::File`). The file must point to a valid ELF binary.
    /// - `name`: An object that can be converted into a `String`, typically a `&str`, which represents the library name.
    ///
    /// # Returns
    /// This function returns a `Result` containing an `ELFLibrary` instance if successful.
    /// If the file is not a valid ELF binary or cannot be loaded, it returns an `Err` containing an error.
    ///
    /// # Safety
    /// This function is safe to call, but the resulting `ELFLibrary` instance should be used
    /// carefully, as incorrect usage may lead to undefined behavior.
    ///
    /// # Examples
    /// ```
    /// use std::fs::File;
    /// use dlopen_rs::ELFLibrary;
    ///
    /// let file = File::open("path_to_elf").unwrap();
    /// let lib = ELFLibrary::from_open_file(file, "my_elf_library").unwrap();
    /// ```
    ///
    /// # Errors
    /// Returns an error if the ELF file cannot be loaded or if there is an issue with the file handle.
    #[cfg(feature = "std")]
    pub fn from_open_file(file: std::fs::File, name: impl AsRef<str>) -> Result<ELFLibrary> {
        let mut file = super::dso::file::ELFFile::new(file);
        let inner = file.load(CString::new(name.as_ref()).unwrap())?;
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
    pub fn from_binary(bytes: &[u8], name: impl AsRef<str>) -> Result<ELFLibrary> {
        let mut file = ELFBinary::new(bytes);
        let inner = file.load(CString::new(name.as_ref()).unwrap())?;
        Ok(ELFLibrary { inner })
    }

    /// get the name of the dependent libraries
    pub fn needed_libs(&self) -> &Vec<&str> {
        &self.inner.needed_libs
    }

    #[inline]
    pub(crate) fn extra_data(&self) -> &ExtraData {
        &self.inner.extra
    }

    #[inline]
    pub(crate) fn relocation(&self) -> &ELFRelocation {
        &self.inner.relocation
    }

    pub(crate) fn relro(&self) -> &Option<ELFRelro> {
        &self.inner.relro
    }

    #[inline]
    pub(crate) fn init_fn(&self) -> &Option<extern "C" fn()> {
        &self.inner.init_fn
    }

    #[inline]
    pub(crate) fn init_array_fn(&self) -> &Option<&'static [extern "C" fn()]> {
        &self.inner.init_array_fn
    }

    pub(crate) fn symbols(&self) -> &SymbolData {
        &self.inner.symbols
    }

    pub(crate) fn name(&self) -> &str {
        self.inner.name.to_str().unwrap()
    }

    #[inline]
    pub(crate) fn set_user_data(
        &mut self,
        user_data: Option<Box<dyn Fn(&str) -> Option<*const ()>>>,
    ) {
        self.inner.extra.set_user_data(user_data);
    }

    #[inline]
    pub(crate) fn set_dep_libs(&mut self, dep_libs: Vec<RelocatedLibrary>) {
        self.inner.extra.set_dep_libs(dep_libs);
    }
}

/// a dynamic shared object
pub(crate) trait SharedObject: MapSegment {
    /// validate ehdr and get phdrs
    fn parse_ehdr(&mut self) -> crate::Result<(Range<usize>, Vec<u8>)>;
    fn load(&mut self, name: CString) -> Result<ELFLibraryInner> {
        let (phdr_range, phdrs) = self.parse_ehdr()?;
        debug_assert_eq!(phdrs.len() % PHDR_SIZE, 0);
        let phdrs = unsafe {
            core::slice::from_raw_parts(phdrs.as_ptr().cast::<Phdr>(), phdrs.len() / PHDR_SIZE)
        };

        let mut addr_min = usize::MAX;
        let mut addr_max = 0;
        let mut addr_min_off = 0;
        let mut addr_min_prot = 0;

        for phdr in phdrs.iter() {
            if phdr.p_type == PT_LOAD {
                let addr_start = phdr.p_vaddr as usize;
                let addr_end = (phdr.p_vaddr + phdr.p_memsz) as usize;
                if addr_start < addr_min {
                    addr_min = addr_start;
                    addr_min_off = phdr.p_offset as usize;
                    addr_min_prot = phdr.p_flags;
                }
                if addr_end > addr_max {
                    addr_max = addr_end;
                }
            }
        }

        addr_max += PAGE_SIZE - 1;
        addr_max &= MASK;
        addr_min &= MASK as usize;
        addr_min_off &= MASK;

        let size = addr_max - addr_min;
        let segments = self.create_segments(addr_min, size, addr_min_off, addr_min_prot)?;
        let base = segments.base();
        let mut unwind = None;
        let mut dynamics = None;
        let mut relro = None;
        #[cfg(feature = "tls")]
        let mut tls = None;
        let mut loaded_phdrs: Option<&[Phdr]> = None;

        for phdr in phdrs {
            match phdr.p_type {
                PT_LOAD => self.load_segment(&segments, phdr)?,
                PT_DYNAMIC => {
                    dynamics = Some(ELFRawDynamic::new((phdr.p_vaddr as usize + base) as _)?)
                }
                PT_GNU_EH_FRAME => unwind = Some(EhFrame::new(phdr, &segments)?),
                PT_GNU_RELRO => relro = Some(ELFRelro::new(phdr, &segments)),
                #[cfg(feature = "tls")]
                PT_TLS => tls = Some(unsafe { tls::ELFTLS::new(phdr, &segments)? }),
                PT_PHDR => {
                    loaded_phdrs = Some(unsafe {
                        core::slice::from_raw_parts(
                            segments.as_mut_ptr().add(phdr.p_vaddr as _).cast(),
                            phdr.p_memsz as usize / size_of::<Phdr>(),
                        )
                    })
                }
                _ => {}
            }
        }

        loaded_phdrs.or_else(|| {
            for phdr in phdrs {
                let cur_range = phdr.p_offset as usize..(phdr.p_offset + phdr.p_filesz) as usize;
                if cur_range.contains(&phdr_range.start) && cur_range.contains(&phdr_range.end) {
                    return Some(unsafe {
                        core::slice::from_raw_parts(
                            segments
                                .as_mut_ptr()
                                .add(phdr_range.start - cur_range.start)
                                .cast(),
                            (cur_range.end - cur_range.start) / size_of::<Phdr>(),
                        )
                    });
                }
            }
            None
        });

        if let Some(unwind_info) = unwind.as_ref() {
            unwind_info.register_unwind(&segments);
        }

        let dynamics = dynamics
            .ok_or(parse_dynamic_error("elf file does not have dynamic"))?
            .finish(base);
        let relocation = ELFRelocation {
            pltrel: dynamics.pltrel(),
            rel: dynamics.rela(),
        };
        #[cfg(feature = "version")]
        let version = dynamics.version_idx().map(|version_idx| {
            version::ELFVersion::new(
                version_idx + base,
                dynamics.verneed().map(|(off, num)| (off + base, num)),
                dynamics.verdef().map(|(off, num)| (off + base, num)),
                &dynamics.strtab(),
            )
        });
        #[cfg(feature = "debug")]
        let link_map = unsafe {
            crate::loader::debug::dl_debug_init(segments.base(), name.as_ptr(), dynamics.addr())
        };
        let elf_lib = ELFLibraryInner {
            name,
            symbols: SymbolData {
                hashtab: dynamics.hashtab(),
                symtab: dynamics.symtab(),
                strtab: dynamics.strtab(),
                #[cfg(feature = "version")]
                version,
            },
            #[cfg(feature = "debug")]
            link_map,
            extra: ExtraData {
                #[cfg(feature = "std")]
                phdrs: loaded_phdrs,
                unwind,
                segments,
                fini_fn: dynamics.fini_fn(),
                fini_array_fn: dynamics.fini_array_fn(),
                #[cfg(feature = "tls")]
                tls,
                user_data: None,
                dep_libs: None,
            },
            relro,
            relocation,
            init_fn: dynamics.init_fn(),
            init_array_fn: dynamics.init_array_fn(),
            needed_libs: dynamics.needed_libs(),
        };
        Ok(elf_lib)
    }
}

pub(crate) trait MapSegment {
    fn create_segments(
        &self,
        addr_min: usize,
        size: usize,
        offset: usize,
        prot: u32,
    ) -> Result<ELFSegments>;
    fn load_segment(&mut self, segments: &ELFSegments, phdr: &Phdr) -> Result<()>;
}

#[cfg(not(feature = "mmap"))]
fn create_segments_impl(addr_min: usize, size: usize) -> crate::Result<ELFSegments> {
    use alloc::alloc::handle_alloc_error;
    use alloc::alloc::Layout;
    use core::ptr::NonNull;
    use segment::ALIGN;
    let len = size + PAGE_SIZE;

    // 鉴于有些平台无法分配出PAGE_SIZE对齐的内存，因此这里分配大一些的内存手动对齐
    let layout = unsafe { Layout::from_size_align_unchecked(len, ALIGN) };
    let memory = unsafe { alloc::alloc::alloc(layout) };
    if memory.is_null() {
        handle_alloc_error(layout);
    }
    let align_offset = ((memory as usize + PAGE_SIZE - 1) & MASK) - memory as usize;

    let offset = align_offset as isize - addr_min as isize;

    // use this set prot to test no_mmap
    // unsafe {
    //     nix::sys::mman::mprotect(
    //         std::ptr::NonNull::new_unchecked(memory.byte_offset(offset) as _),
    //         len,
    //         nix::sys::mman::ProtFlags::PROT_EXEC
    //             | nix::sys::mman::ProtFlags::PROT_READ
    //             | nix::sys::mman::ProtFlags::PROT_WRITE,
    //     )
    //     .unwrap()
    // };

    let memory = unsafe { NonNull::new_unchecked(memory as _) };
    Ok(ELFSegments::new(memory, offset, len))
}
