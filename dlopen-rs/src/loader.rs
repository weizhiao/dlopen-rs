use crate::{
    dynamic::ELFDynamic,
    file::{Buf, ELFFile},
    hashtable::ELFHashTable,
    parse_dynamic_error,
    relocation::ELFRelocation,
    segment::{ELFRelro, ELFSegments},
    types::{CommonInner, ELFLibraryInner},
    unwind::ELFUnwind,
    Result,
};
use alloc::vec::Vec;
use elf::abi::*;

impl ELFLibraryInner {
    pub(crate) fn load_library(mut file: ELFFile) -> Result<ELFLibraryInner> {
        #[cfg(feature = "std")]
        let mut buf = Buf::new();
        #[cfg(not(feature = "std"))]
        let mut buf = Buf;
        let phdrs = file.parse_phdrs(&mut buf)?;
        let segments = ELFSegments::new(phdrs, &file)?;

        let mut unwind = None;
        let mut dynamics = None;
        let mut relro = None;
        #[cfg(feature = "tls")]
        let mut tls = None;

        for phdr in phdrs {
            match phdr.p_type {
                PT_LOAD => segments.load_segment(phdr, &mut file)?,
                PT_DYNAMIC => dynamics = Some(ELFDynamic::new(phdr, &segments)),
                PT_GNU_EH_FRAME => unwind = Some(ELFUnwind::new(phdr, &segments)?),
                PT_GNU_RELRO => relro = Some(ELFRelro::new(phdr, &segments)),
                #[cfg(feature = "tls")]
                PT_TLS => {
                    tls = Some(Box::new(unsafe {
                        crate::tls::ELFTLS::new(phdr, &segments)?
                    }))
                }
                _ => {}
            }
        }

        if let Some(unwind_info) = unwind.as_ref() {
            unwind_info.register_unwind(&segments);
        }

        let dynamics = if let Some(dynamics) = dynamics {
            dynamics
        } else {
            return Err(parse_dynamic_error("elf file does not have dynamic"));
        }?;

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
            common: CommonInner {
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
            relocation,
            init_fn: dynamics.init_fn(),
            init_array_fn: dynamics.init_array_fn(),
            needed_libs,
        };
        Ok(elf_lib)
    }
}
