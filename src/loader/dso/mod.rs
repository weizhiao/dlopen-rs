pub(crate) mod binary;
mod dynamic;
mod ehdr;
mod ehframe;
#[cfg(feature = "std")]
pub(crate) mod file;
mod hash_table;
mod segment;
mod string_table;
#[cfg(feature = "tls")]
pub(crate) mod tls;

use alloc::{
    boxed::Box,
    ffi::CString,
    string::{String, ToString},
};
use core::{ffi::CStr, ops::Range};
use string_table::ELFStringTable;

use super::{
    arch::{ELFSymbol, Phdr, Rela, PHDR_SIZE},
    ExternLibrary, RelocatedLibrary,
};

use crate::{parse_dynamic_error, Result};
use alloc::vec::Vec;
use binary::ELFBinary;
use dynamic::ELFDynamic;
use ehframe::EhFrame;
use elf::abi::*;
use hash_table::ELFHashTable;
use segment::{ELFRelro, ELFSegments, MASK, PAGE_SIZE};

#[derive(Debug)]
pub(crate) struct ELFRelocation {
    pub(crate) pltrel: Option<&'static [Rela]>,
    pub(crate) rel: Option<&'static [Rela]>,
}

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct InternalLib {
    common: CommonElfData,
    #[cfg(feature = "std")]
    is_register: core::sync::atomic::AtomicBool,
    internal_libs: Box<[RelocatedLibrary]>,
    external_libs: Option<Box<[Box<dyn ExternLibrary>]>>,
}

impl InternalLib {
    pub(crate) fn new(
        lib: ELFLibrary,
        internal_libs: Vec<RelocatedLibrary>,
        external_libs: Option<Vec<Box<dyn ExternLibrary>>>,
    ) -> InternalLib {
        InternalLib {
            common: lib.into_common_data(),
            #[cfg(feature = "std")]
            is_register: core::sync::atomic::AtomicBool::new(false),
            internal_libs: internal_libs.into_boxed_slice(),
            external_libs: external_libs.map(|libs| libs.into_boxed_slice()),
        }
    }

    #[cfg(feature = "std")]
    pub(crate) fn is_register(&self) -> bool {
        self.is_register.load(core::sync::atomic::Ordering::SeqCst)
    }

    #[cfg(feature = "std")]
    /// set is_register true
    pub(crate) fn register(&self) {
        self.is_register
            .store(true, core::sync::atomic::Ordering::SeqCst);
    }

    pub(crate) fn get_sym(&self, name: &str) -> Option<&ELFSymbol> {
        self.common.get_sym(name)
    }

    pub(crate) fn common_data(&self) -> &CommonElfData {
        &self.common
    }

    pub(crate) fn name(&self) -> &CStr {
        self.common.name()
    }
}

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct CommonElfData {
    /// file name
    name: CString,
    /// phdrs
    #[cfg(feature = "std")]
    phdrs: Option<&'static [Phdr]>,
    /// .gnu.hash
    hashtab: ELFHashTable,
    /// .dynsym
    symtab: *const ELFSymbol,
    /// .dynstr
    strtab: ELFStringTable<'static>,
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
}

impl CommonElfData {
    pub(crate) fn get_sym(&self, name: &str) -> Option<&ELFSymbol> {
        let symbol = unsafe {
            self.hashtab
                .find(name.as_bytes(), self.symtab, &self.strtab)
        };
        symbol
    }

    #[cfg(feature = "std")]
    pub(crate) fn phdrs(&self) -> Option<&[Phdr]> {
        self.phdrs
    }

    pub(crate) fn name(&self) -> &CStr {
        &self.name
    }

    #[inline]
    pub(crate) fn base(&self) -> usize {
        self.segments.base()
    }

    #[inline]
    pub(crate) fn symtab(&self) -> *const ELFSymbol {
        self.symtab
    }

    #[inline]
    pub(crate) fn strtab(&self) -> &ELFStringTable {
        &self.strtab
    }

    #[inline]
    pub(crate) fn fini_fn(&self) -> &Option<extern "C" fn()> {
        &self.fini_fn
    }

    #[inline]
    pub(crate) fn fini_array_fn(&self) -> &Option<&'static [extern "C" fn()]> {
        &self.fini_array_fn
    }

    #[inline]
    #[cfg(feature = "tls")]
    pub(crate) fn tls(&self) -> Option<*const tls::ELFTLS> {
        self.tls.as_ref().map(|val| val.as_ref() as _)
    }
}

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct ELFLibraryInner {
    common: CommonElfData,
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

#[derive(Debug)]
pub struct ELFLibrary {
    inner: ELFLibraryInner,
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
        let mut file = super::dso::file::ELFFile::new(path)?;
        let inner = file.load(file_name)?;
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
    pub fn needed_libs(&self) -> &Vec<&str> {
        &self.inner.needed_libs
    }

    #[inline]
    pub(crate) fn common_data(&self) -> &CommonElfData {
        &self.inner.common
    }

    #[inline]
    pub(crate) fn into_common_data(self) -> CommonElfData {
        self.inner.common
    }

    #[inline]
    pub(crate) fn relocation(&self) -> &ELFRelocation {
        &self.inner.relocation
    }

    pub(crate) fn relro(&self) -> &Option<ELFRelro> {
        &self.inner.relro
    }

    #[inline]
    pub(crate) fn symtab(&self) -> *const ELFSymbol {
        self.inner.common.symtab()
    }

    #[inline]
    pub(crate) fn strtab(&self) -> &ELFStringTable {
        &self.inner.common.strtab()
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
                PT_DYNAMIC => dynamics = Some(ELFDynamic::new(phdr, &segments)?),
                PT_GNU_EH_FRAME => unwind = Some(EhFrame::new(phdr, &segments)?),
                PT_GNU_RELRO => relro = Some(ELFRelro::new(phdr, &segments)),
                #[cfg(feature = "tls")]
                PT_TLS => tls = Some(Box::new(unsafe { tls::ELFTLS::new(phdr, &segments)? })),
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
                    debug_assert_eq!((cur_range.end - cur_range.start) % size_of::<Phdr>(), 0);
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

        let dynamics = dynamics.ok_or(parse_dynamic_error("elf file does not have dynamic"))?;

        let strtab = ELFStringTable::new(dynamics.strtab());

        let needed_libs: Vec<&'static str> = dynamics
            .needed_libs()
            .iter()
            .map(|needed_lib| strtab.get_str(*needed_lib).unwrap())
            .collect();

        let relocation = ELFRelocation {
            pltrel: dynamics.pltrel(),
            rel: dynamics.rela(),
        };

        let hashtab = ELFHashTable::parse_gnu_hash(dynamics.hash() as _);
        let symtab = dynamics.dynsym() as _;

        let elf_lib = ELFLibraryInner {
            common: CommonElfData {
                name: CString::new(name).unwrap(),
                #[cfg(feature = "std")]
                phdrs: loaded_phdrs,
                hashtab,
                symtab,
                strtab,
                unwind,
                segments,
                fini_fn: dynamics.fini_fn(),
                fini_array_fn: dynamics.fini_array_fn(),
                #[cfg(feature = "tls")]
                tls,
            },
            relro,
            relocation,
            init_fn: dynamics.init_fn(),
            init_array_fn: dynamics.init_array_fn(),
            needed_libs,
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
