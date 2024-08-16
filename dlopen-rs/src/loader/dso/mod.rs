pub(crate) mod binary;
mod dynamic;
mod ehdr;
mod ehframe;
#[cfg(feature = "std")]
pub(crate) mod file;
mod hashtable;
#[cfg(feature = "load_self")]
mod load_self;
mod segment;
#[cfg(feature = "tls")]
pub(crate) mod tls;

use super::arch::{ELFSymbol, Phdr, Rela, PHDR_SIZE};

use crate::{parse_dynamic_error, Result};
use alloc::vec::Vec;
use binary::ELFBinary;
use dynamic::ELFDynamic;
use ehframe::ELFUnwind;
use elf::{abi::*, string_table::StringTable};
use hashtable::ELFHashTable;
use segment::{ELFRelro, ELFSegments, MASK, PAGE_SIZE};

#[derive(Debug)]
pub(crate) struct ELFRelocation {
    pub(crate) pltrel: Option<&'static [Rela]>,
    pub(crate) rel: Option<&'static [Rela]>,
}

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct CommonElfData {
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
    pub(crate) tls: Option<Box<tls::ELFTLS>>,
}

impl CommonElfData {
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
#[allow(unused)]
pub(crate) struct ELFLibraryInner {
    pub(crate) common: CommonElfData,
    /// rela.dyn and rela.plt
    pub(crate) relocation: ELFRelocation,
    /// GNU_RELRO segment
    pub(crate) relro: Option<ELFRelro>,
    /// .init
    pub(crate) init_fn: Option<extern "C" fn()>,
    /// .init_array
    pub(crate) init_array_fn: Option<&'static [extern "C" fn()]>,
    /// needed libs' name
    pub(crate) needed_libs: Vec<&'static str>,
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
        let mut file = super::dso::file::ELFFile::new(path)?;
        let inner = file.load()?;
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
        let mut file = ELFBinary::new(bytes);
        let inner = file.load()?;
        Ok(ELFLibrary { inner })
    }

    /// get the name of the dependent libraries
    pub fn needed_libs(&self) -> &Vec<&str> {
        &self.inner.needed_libs
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
    pub(crate) fn tls(&self) -> *const tls::ELFTLS {
        self.inner.common.tls.as_ref().unwrap().as_ref() as *const tls::ELFTLS
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
    fn parse_ehdr(&mut self) -> crate::Result<Vec<u8>>;
    fn load(&mut self) -> Result<ELFLibraryInner> {
        let phdrs = self.parse_ehdr()?;
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

        for phdr in phdrs {
            match phdr.p_type {
                PT_LOAD => self.load_segment(&segments, phdr)?,
                PT_DYNAMIC => dynamics = Some(ELFDynamic::new(phdr, &segments)?),
                PT_GNU_EH_FRAME => unwind = Some(ELFUnwind::new(phdr, &segments)?),
                PT_GNU_RELRO => relro = Some(ELFRelro::new(phdr, &segments)),
                #[cfg(feature = "tls")]
                PT_TLS => tls = Some(Box::new(unsafe { tls::ELFTLS::new(phdr, &segments)? })),
                _ => {}
            }
        }

        if let Some(unwind_info) = unwind.as_ref() {
            unwind_info.register_unwind(&segments);
        }

        let dynamics = dynamics.ok_or(parse_dynamic_error("elf file does not have dynamic"))?;

        let strtab = elf::string_table::StringTable::new(dynamics.strtab());

        let needed_libs: Vec<&'static str> = dynamics
            .needed_libs()
            .iter()
            .map(|needed_lib| strtab.get(*needed_lib).unwrap())
            .collect();

        let relocation = ELFRelocation {
            pltrel: dynamics.pltrel(),
            rel: dynamics.rela(),
        };

        let hashtab = ELFHashTable::parse_gnu_hash(dynamics.hash() as _);
        let symtab = dynamics.dynsym() as _;

        let elf_lib = ELFLibraryInner {
            common: CommonElfData {
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
fn create_segments_impl(
    addr_min: usize,
    size: usize,
) -> crate::Result<ELFSegments> {
    use alloc::alloc::handle_alloc_error;
    use alloc::alloc::Layout;
    use segment::ALIGN;
    use core::ptr::NonNull;
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
    Ok(ELFSegments {
        memory,
        offset,
        len,
    })
}
