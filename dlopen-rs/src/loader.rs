use crate::{
    dynamic::ELFDynamic,
    elfloader_error,
    file::{Buf, ELFFile},
    hash::ELFHashTable,
    relocation::ELFRelocation,
    segment::{ELFRelro, ELFSegments},
    unwind::ELFUnwind,
    ELFSymbol, Result,
};
use elf::abi::*;

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct ELFLibraryInner {
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
    /// rela.dyn and rela.plt
    pub(crate) relocation: ELFRelocation,
    /// .init
    pub(crate) init_fn: Option<extern "C" fn()>,
    /// .init_array
    pub(crate) init_array_fn: Option<&'static [extern "C" fn()]>,
    /// .fini
    pub(crate) fini_fn: Option<extern "C" fn()>,
    /// .fini_array
    pub(crate) fini_array_fn: Option<&'static [extern "C" fn()]>,
    /// needed libs' name
    pub(crate) needed_libs: Vec<&'static str>,
    /// .tbss and .tdata
    #[cfg(feature = "tls")]
    pub(crate) tls: Option<Box<crate::tls::ELFTLS>>,
}

impl ELFLibraryInner {
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

    pub(crate) fn load_library(mut file: ELFFile) -> Result<ELFLibraryInner> {
        #[cfg(feature = "std")]
        let mut buf = Buf::new();
        #[cfg(not(feature = "std"))]
        let mut buf = Buf;
        let phdrs = file.parse_phdrs(&mut buf)?;
        let segments = ELFSegments::new(phdrs, &file)?;

        #[cfg(feature = "unwinding")]
        let mut text_start = usize::MAX;
        #[cfg(feature = "unwinding")]
        let mut text_end = usize::MAX;

        let mut unwind = None;
        let mut dynamics = None;
        let mut relro = None;
        #[cfg(feature = "tls")]
        let mut tls = None;

        for phdr in phdrs {
            match phdr.p_type {
                PT_LOAD => {
                    segments.load_segment(phdr, &file)?;
                    #[cfg(feature = "unwinding")]
                    {
                        if phdr.p_flags & PF_X != 0 {
                            let this_min = phdr.p_vaddr as usize - addr_min;
                            let this_max = (phdr.p_vaddr + phdr.p_filesz) as usize - addr_min;
                            text_start = this_min + base;
                            text_end = this_max + base;
                        }
                    }
                }
                PT_DYNAMIC => dynamics = Some(ELFDynamic::new(phdr, &segments)),
                PT_GNU_EH_FRAME => unwind = Some(ELFUnwind::new(phdr, &segments)?),
                PT_GNU_RELRO => relro = Some(ELFRelro::new(phdr, &segments)),
                #[cfg(feature = "tls")]
                PT_TLS => {
                    tls = Some(Box::new(unsafe {
                        crate::tls::ELFTLS::new(phdr, &segments)
                    }))
                }
                _ => {}
            }
        }

        let dynamics = if let Some(dynamics) = dynamics {
            dynamics
        } else {
            return elfloader_error("elf file does not have dynamic");
        }?;

        #[cfg(feature = "unwinding")]
        if unlikely(text_start == usize::MAX || text_start == text_end) {
            return elfloader_error("can not find .text start");
        }

        let strtab = elf::string_table::StringTable::new(dynamics.strtab());

        let needed_libs: Vec<&'static str> = dynamics
            .needed_libs()
            .iter()
            .map(|needed_lib| strtab.get(*needed_lib).unwrap())
            .collect();

        let relocation = ELFRelocation {
            pltrel: dynamics.pltrel(),
            rel: dynamics.rela(),
            relro,
        };

        let hashtab = ELFHashTable::parse_gnu_hash(dynamics.hash() as _);
        let symtab = dynamics.dynsym() as _;

        let elf_lib = ELFLibraryInner {
            segments,
            hashtab,
            symtab,
            strtab,
            unwind,
            relocation,
            init_fn: dynamics.init_fn(),
            init_array_fn: dynamics.init_array_fn(),
            fini_fn: dynamics.fini_fn(),
            fini_array_fn: dynamics.fini_array_fn(),
            needed_libs,
            #[cfg(feature = "tls")]
            tls,
        };
        Ok(elf_lib)
    }
}
