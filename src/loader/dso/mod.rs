mod binary;
pub(crate) mod dynamic;
mod ehdr;
mod ehframe;
#[cfg(feature = "std")]
mod file;
pub(crate) mod hashtab;
mod segment;
pub(crate) mod strtab;
#[cfg(feature = "tls")]
pub(crate) mod tls;
pub(crate) mod version;

use alloc::{
    ffi::CString,
    string::{String, ToString},
};
use core::{fmt::Debug, ops::Range};
use strtab::ELFStringTable;
use version::{ELFVersion, SymbolRequireVersion};

use super::{
    arch::{ELFSymbol, Phdr, Rela, PHDR_SIZE},
    LibraryExtraData, RelocatedLibrary, RelocatedLibraryInner,
};

use crate::{parse_dynamic_error, Result};
use alloc::vec::Vec;
use binary::ELFBinary;
use dynamic::ELFRawDynamic;
use ehframe::EhFrame;
use elf::abi::*;
use hashtab::ELFGnuHash;
use segment::{ELFRelro, ELFSegments, MASK, PAGE_SIZE};

#[derive(Debug)]
pub(crate) struct ELFRelocation {
    pub(crate) pltrel: Option<&'static [Rela]>,
    pub(crate) rel: Option<&'static [Rela]>,
}

#[derive(Debug)]
pub(crate) struct SymbolData {
    /// .gnu.hash
    pub hashtab: ELFGnuHash,
    /// .dynsym
    pub symtab: *const ELFSymbol,
    /// .dynstr
    pub strtab: ELFStringTable<'static>,
    /// .gnu.version
    pub version: Option<ELFVersion>,
}

impl SymbolData {
    pub(crate) fn get_sym(
        &self,
        name: &str,
        version: &Option<SymbolRequireVersion>,
    ) -> Option<(&ELFSymbol, usize)> {
        let symbol = unsafe {
            self.hashtab
                .find(name.as_bytes(), self.symtab, &self.strtab)
        };
        symbol
    }

    pub(crate) fn rel_symbol(
        &self,
        idx: usize,
    ) -> (&ELFSymbol, &str, Option<SymbolRequireVersion>) {
        let symbol = unsafe { &*self.symtab.add(idx) };
        let name = self.strtab.get_str(symbol.st_name as usize).unwrap();
        let require = self.get_requirement(idx);
        (symbol, name, require)
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
    /// debug link map
    #[cfg(feature = "debug")]
    link_map: super::debug::DebugInfo,
    /// user data
    user_data: Option<Box<dyn Fn(&str) -> Option<*const ()>>>,
    /// dependency libraries
    dep_libs: Option<Vec<RelocatedLibrary>>,
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
    /// common elf data
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

pub struct ELFLibrary {
    inner: ELFLibraryInner,
}

impl From<ELFLibrary> for RelocatedLibraryInner {
    fn from(value: ELFLibrary) -> Self {
        RelocatedLibraryInner {
            name: value.inner.name,
            base: value.inner.extra.base(),
            symbols: value.inner.symbols,
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
        let file_name = path.as_ref().to_str().unwrap().to_string();
        let file = std::fs::File::open(path.as_ref())?;
        let mut file = super::dso::file::ELFFile::new(file);
        let inner = file.load(file_name)?;
        Ok(ELFLibrary { inner })
    }

    #[cfg(feature = "std")]
    pub fn from_open_file(file: std::fs::File, name: impl ToString) -> Result<ELFLibrary> {
        let mut file = super::dso::file::ELFFile::new(file);
        let inner = file.load(name.to_string())?;
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
    pub fn from_binary(bytes: &[u8], name: impl ToString) -> Result<ELFLibrary> {
        let mut file = ELFBinary::new(bytes);
        let inner = file.load(name.to_string())?;
        Ok(ELFLibrary { inner })
    }

    /// get the name of the dependent libraries
    pub fn needed_libs(&mut self) -> Vec<&str> {
        core::mem::take(&mut self.inner.needed_libs)
    }

    #[inline]
    pub(crate) fn common_data(&self) -> &ExtraData {
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
    fn load(&mut self, name: String) -> Result<ELFLibraryInner> {
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
                    dynamics = Some(ELFRawDynamic::new(
                        (phdr.p_vaddr as usize + segments.base()) as _,
                    )?)
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
            .finish(segments.base());
        let name = CString::new(name).unwrap();
        let relocation = ELFRelocation {
            pltrel: dynamics.pltrel(),
            rel: dynamics.rela(),
        };
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
                version: dynamics.version(),
            },
            extra: ExtraData {
                #[cfg(feature = "std")]
                phdrs: loaded_phdrs,
                unwind,
                segments,
                fini_fn: dynamics.fini_fn(),
                fini_array_fn: dynamics.fini_array_fn(),
                #[cfg(feature = "tls")]
                tls,
                #[cfg(feature = "debug")]
                link_map,
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
