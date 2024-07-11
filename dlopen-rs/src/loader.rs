use std::path::Path;

use crate::{
    dynamic::ELFDynamic,
    elfloader_error,
    file::{Buf, ELFFile},
    hash::ELFHashTable,
    relocation::{ELFRelas, ELFRelro},
    segment::ELFSegments,
    unlikely,
    unwind::UnwindInfo,
    Result, Symbol, MASK, PAGE_SIZE,
};
use elf::abi::*;

#[derive(Debug)]
#[allow(unused)]
pub struct ELFLibrary {
    pub(crate) hashtab: ELFHashTable,
    //.dynsym
    pub(crate) symtab: *const Symbol,
    //.dynstr
    pub(crate) strtab: elf::string_table::StringTable<'static>,
    // 保存unwind信息,UnwindInfo一定要先于ELFMemory drop,
    // 因为__deregister_frame注销时会使用elf文件中eh_frame的地址
    // 一旦memory被销毁了，访问该地址就会发生段错误
    unwind_info: Option<UnwindInfo>,
    //elflibrary在内存中的映射
    pub(crate) segments: ELFSegments,
    pub(crate) rela_sections: ELFRelas,
    init_fn: Option<extern "C" fn()>,
    init_array_fn: Option<&'static [extern "C" fn()]>,
}

impl ELFLibrary {

    pub fn from_file(path: &Path) -> Result<ELFLibrary> {
        let file = ELFFile::from_file(path)?;
        Self::load_library(file)
    }

    pub fn from_binary(bytes: &[u8]) -> Result<ELFLibrary> {
        let file = ELFFile::from_binary(bytes);
        Self::load_library(file)
    }

    pub(crate) fn load_library(mut file: ELFFile) -> Result<ELFLibrary> {
        //通常来说ehdr的后面就是phdrs，因此这里假设ehdr后面就是phdrs，会多读8个phdr的大小，若符合假设则可以减少一次系统调用
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

        //可能没有重定位段
        let rela_sections = ELFRelas {
            pltrel: dynamics.pltrel(),
            rel: dynamics.rela(),
            relro,
        };

        let hashtab = ELFHashTable::parse_gnu_hash(dynamics.hash() as _);
        let symtab = dynamics.dynsym() as _;

        if let Some(unwind_info) = &unwind_info {
            unwind_info.register_unwind_info();
        }

        let elf_lib = ELFLibrary {
            segments,
            hashtab,
            symtab,
            strtab,
            unwind_info,
            rela_sections,
            init_fn: dynamics.init_fn(),
            init_array_fn: dynamics.init_array_fn(),
        };
        Ok(elf_lib)
    }

    pub fn do_init(&self) {
        if let Some(init) = self.init_fn {
            init();
        }

        if let Some(init_array) = self.init_array_fn {
            for init in init_array {
                init();
            }
        }
    }
}
