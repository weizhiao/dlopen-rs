mod arch;
mod binary;
mod dynamic;
mod ehframe;
#[cfg(feature = "std")]
mod file;
pub mod mmap;
mod relocation;
mod segment;
mod symbol;
#[cfg(feature = "tls")]
mod tls;
mod types;
#[cfg(feature = "version")]
mod version;

use crate::{parse_dynamic_error, RelocatedLibrary, Result};
use alloc::ffi::CString;
use alloc::vec::Vec;
use arch::{Phdr, Rela};
use binary::ELFBinary;
use core::{fmt::Debug, ops::Range};
use ehframe::EhFrame;
use elf::abi::*;
use mmap::Mmap;
use relocation::UserData;
use segment::{ELFRelro, ELFSegments, MapSegment, MASK};
use types::ELFRelocation;

pub(crate) use arch::*;
pub(crate) use dynamic::ELFRawDynamic;
pub use segment::PAGE_SIZE;
pub(crate) use symbol::{SymbolData, SymbolInfo};
#[cfg(feature = "tls")]
pub(crate) use tls::tls_get_addr;
pub(crate) use types::ExtraData;
#[cfg(feature = "version")]
pub(crate) use version::SymbolVersion;

#[allow(unused)]
pub(crate) struct ELFLibraryInner {
    /// file name
    pub(crate) name: CString,
    /// elf symbols
    pub(crate) symbols: SymbolData,
    /// debug link map
    #[cfg(feature = "debug")]
    pub(crate) link_map: super::debug::DebugInfo,
    /// extra elf data
    pub(crate) extra: ExtraData,
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
    pub(crate) inner: ELFLibraryInner,
}

impl Debug for ELFLibrary {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.inner.fmt(f)
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
    /// let lib = ELFLibrary::from_file::<MmapImpl>("/path/to/awesome.module")
    ///		.unwrap();
    /// ```
    ///
    #[cfg(feature = "std")]
    pub fn from_file<M: Mmap>(path: impl AsRef<std::ffi::OsStr>) -> Result<Self> {
        let path = path.as_ref();
        let file_name = path.to_str().unwrap();
        let file = std::fs::File::open(path)?;
        let mut file = file::ELFFile::new(file);
        let inner = file.load::<M>(CString::new(file_name).unwrap())?;
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
    /// let lib = ELFLibrary::from_open_file::<MmapImpl>(file, "my_elf_library").unwrap();
    /// ```
    ///
    /// # Errors
    /// Returns an error if the ELF file cannot be loaded or if there is an issue with the file handle.
    #[cfg(feature = "std")]
    pub fn from_open_file<M: Mmap>(
        file: std::fs::File,
        name: impl AsRef<str>,
    ) -> Result<ELFLibrary> {
        let mut file = file::ELFFile::new(file);
        let inner = file.load::<M>(CString::new(name.as_ref()).unwrap())?;
        Ok(ELFLibrary { inner })
    }

    /// load a elf dynamic library from bytes
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let path = Path::new("/path/to/awesome.module");
    /// let bytes = std::fs::read(path).unwrap();
    /// let lib = ELFLibrary::from_binary::<MmapImpl>(&bytes).unwarp();
    /// ```
    pub fn from_binary<M: Mmap>(bytes: &[u8], name: impl AsRef<str>) -> Result<Self> {
        let mut file = ELFBinary::new(bytes);
        let inner = file.load::<M>(CString::new(name.as_ref()).unwrap())?;
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
    pub(crate) fn relocation(&mut self) -> ELFRelocation {
        core::mem::take(&mut self.inner.relocation)
    }

    pub fn is_finished(&self) -> bool {
        self.inner.relocation.is_finished()
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
    pub(crate) fn user_data(&mut self) -> &mut UserData {
        self.inner.extra.user_data()
    }

    #[inline]
    pub(crate) fn insert_dep_libs(&mut self, dep_libs: impl AsRef<[RelocatedLibrary]>) {
        self.inner.extra.insert_dep_libs(dep_libs);
    }

    #[inline]
    pub(crate) fn set_relocation(&mut self, relocation: ELFRelocation) {
        self.inner.relocation = relocation;
    }
}

/// a dynamic shared object
pub(crate) trait SharedObject: MapSegment {
    /// validate ehdr and get phdrs
    fn parse_ehdr(&mut self) -> crate::Result<(Range<usize>, Vec<u8>)>;
    fn load<M: Mmap>(&mut self, name: CString) -> Result<ELFLibraryInner> {
        let (phdr_range, phdrs) = self.parse_ehdr()?;
        debug_assert_eq!(phdrs.len() % PHDR_SIZE, 0);
        let phdrs = unsafe {
            core::slice::from_raw_parts(phdrs.as_ptr().cast::<Phdr>(), phdrs.len() / PHDR_SIZE)
        };

        let mut min_vaddr = usize::MAX;
        let mut max_vaddr = 0;
        // 最小偏移地址对应内容在文件中的偏移
        let mut min_off = 0;
        let mut min_size = 0;
        let mut min_prot = 0;

        //找到最小的偏移地址和最大的偏移地址
        for phdr in phdrs.iter() {
            if phdr.p_type == PT_LOAD {
                let vaddr_start = phdr.p_vaddr as usize;
                let vaddr_end = (phdr.p_vaddr + phdr.p_memsz) as usize;
                if vaddr_start < min_vaddr {
                    min_vaddr = vaddr_start;
                    min_off = phdr.p_offset as usize;
                    min_prot = phdr.p_flags;
                    min_size = phdr.p_filesz as usize;
                }
                if vaddr_end > max_vaddr {
                    max_vaddr = vaddr_end;
                }
            }
        }

        // 按页对齐
        max_vaddr = (max_vaddr + PAGE_SIZE - 1) & MASK;
        min_vaddr &= MASK as usize;

        let total_size = max_vaddr - min_vaddr;
        // 创建加载动态库所需的空间，并同时映射min_vaddr对应的segment
        let segments =
            Self::create_segments::<M>(self, min_vaddr, total_size, min_off, min_size, min_prot)?;
        // 获取基地址
        let base = segments.base();
        let mut unwind = None;
        let mut dynamics = None;
        let mut relro = None;
        #[cfg(feature = "tls")]
        let mut tls = None;
        let mut loaded_phdrs: Option<&[Phdr]> = None;

        // 根据Phdr的类型进行不同操作
        for phdr in phdrs {
            match phdr.p_type {
                // 将segment加载到内存中
                PT_LOAD => self.load_segment::<M>(&segments, phdr)?,
                // 解析.dynamic section
                PT_DYNAMIC => {
                    dynamics = Some(ELFRawDynamic::new((phdr.p_vaddr as usize + base) as _)?)
                }
                PT_GNU_EH_FRAME => unwind = Some(EhFrame::new(phdr, &segments)?),
                PT_GNU_RELRO => relro = Some(ELFRelro::new::<M>(phdr, segments.base())),
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

        let dynamics = dynamics
            .ok_or(parse_dynamic_error("elf file does not have dynamic"))?
            .finish(base);
        let relocation = ELFRelocation::new(dynamics.pltrel(), dynamics.dynrel());
        #[cfg(feature = "debug")]
        let link_map =
            unsafe { crate::debug::dl_debug_init(segments.base(), name.as_ptr(), dynamics.addr()) };
        let symbols = SymbolData::new(
            dynamics.hashtab(),
            dynamics.symtab(),
            dynamics.strtab(),
            dynamics.strtab_size(),
            #[cfg(feature = "version")]
            dynamics.version_idx().map(|version_idx| version_idx + base),
            #[cfg(feature = "version")]
            dynamics.verneed().map(|(off, num)| (off + base, num)),
            #[cfg(feature = "version")]
            dynamics.verdef().map(|(off, num)| (off + base, num)),
        );
        let needed_libs: Vec<&'static str> = dynamics
            .needed_libs()
            .iter()
            .map(|needed_lib| symbols.strtab().get(*needed_lib))
            .collect();
        let elf_lib = ELFLibraryInner {
            name,
            symbols,
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
                user_data: UserData::empty(),
                dep_libs: Vec::new(),
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
