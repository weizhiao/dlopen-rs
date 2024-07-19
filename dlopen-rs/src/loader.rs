use crate::{
    dynamic::ELFDynamic,
    elfloader_error,
    file::{Buf, ELFFile},
    hash::ELFHashTable,
    relocation::{ELFRelocation, ELFRelro},
    segment::ELFSegments,
    unlikely,
    unwind::UnwindInfo,
    Result, ELFSymbol, MASK, PAGE_SIZE,
};
use elf::abi::*;

#[derive(Debug)]
#[allow(unused)]
pub(crate) struct ELFLibraryInner {
    pub(crate) hashtab: ELFHashTable,
    //.dynsym
    pub(crate) symtab: *const ELFSymbol,
    //.dynstr
    pub(crate) strtab: elf::string_table::StringTable<'static>,
    // 保存unwind信息,UnwindInfo一定要先于ELFMemory drop,
    // 因为__deregister_frame注销时会使用elf文件中eh_frame的地址
    // 一旦memory被销毁了，访问该地址就会发生段错误
    unwind_info: Option<UnwindInfo>,
    //elflibrary在内存中的映射
    pub(crate) segments: ELFSegments,
    pub(crate) relocation: ELFRelocation,
    pub(crate) init_fn: Option<extern "C" fn()>,
    pub(crate) init_array_fn: Option<&'static [extern "C" fn()]>,
    pub(crate) fini_fn: Option<extern "C" fn()>,
    pub(crate) fini_array_fn: Option<&'static [extern "C" fn()]>,
    needed_libs: Vec<&'static str>,
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

        let mut addr_min = usize::MAX;
        let mut addr_max = 0;
        let mut addr_min_off = 0;
        let mut addr_min_prot = 0;

        for phdr in phdrs {
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

        let len = addr_max - addr_min;
        let segments = ELFSegments::new(addr_min_prot, len, addr_min_off, addr_min, &file)?;

        #[cfg(feature = "unwinding")]
        let mut text_start = usize::MAX;
        #[cfg(feature = "unwinding")]
        let mut text_end = usize::MAX;

        let mut unwind_info = None;
        let mut dynamics = None;
        let mut relro = None;
        #[cfg(feature = "tls")]
        let mut tls = None;

        for phdr in phdrs {
            match phdr.p_type {
                PT_LOAD => {
                    segments.load_segment(&phdr, &file)?;
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
                PT_GNU_EH_FRAME => unwind_info = Some(UnwindInfo::new(phdr, &segments)?),
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

        if unlikely(dynamics.is_none()) {
            return elfloader_error("elf file does not have dynamic");
        }

        #[cfg(feature = "unwinding")]
        if unlikely(text_start == usize::MAX || text_start == text_end) {
            return elfloader_error("can not find .text start");
        }

        let dynamics = dynamics.unwrap()?;
        let strtab = elf::string_table::StringTable::new(dynamics.strtab());

        let needed_libs: Vec<&'static str> = dynamics
            .needed_libs()
            .iter()
            .map(|needed_lib| strtab.get(*needed_lib).unwrap())
            .collect();

        let rela_sections = ELFRelocation {
            pltrel: dynamics.pltrel(),
            rel: dynamics.rela(),
            relro,
        };

        let hashtab = ELFHashTable::parse_gnu_hash(dynamics.hash() as _);
        let symtab = dynamics.dynsym() as _;

        if let Some(unwind_info) = &unwind_info {
            unwind_info.register_unwind_info();
        }

        let elf_lib = ELFLibraryInner {
            segments,
            hashtab,
            symtab,
            strtab,
            unwind_info,
            relocation: rela_sections,
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
