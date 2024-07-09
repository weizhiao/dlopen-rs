use std::path::Path;

use crate::{
    dynamic::ELFDynamic,
    elfloader_error,
    file::{Buf, ELFFile},
    gnuhash::ELFGnuHash,
    relocation::{ELFRelas, ELFRelro},
    segment::ELFSegments,
    unlikely,
    unwind::UnwindInfo,
    Rela, Result, Symbol, MASK, PAGE_SIZE,
};
use elf::abi::*;

#[derive(Debug)]
#[allow(unused)]
pub struct ELFLibrary {
    //.gnu.hash
    hashtab: ELFGnuHash,
    //.dynsym
    pub(crate) symtab: *const Symbol,
    //.dynstr
    pub(crate) strtab: elf::string_table::StringTable<'static>,
    // 保存unwind信息,UnwindInfo一定要先于ELFMemory drop,
    // 因为__deregister_frame注销时会使用elf文件中eh_frame的地址
    // 一旦memory被销毁了，访问该地址就会发生段错误
    unwind_info: UnwindInfo,
    //elflibrary在内存中的映射
    pub(crate) segments: ELFSegments,
    pub(crate) relro: Option<ELFRelro>,
    pub(crate) rela_sections: ELFRelas,
}

impl ELFLibrary {
    pub fn get(&self, name: &str) -> Option<*const ()> {
        let bytes = name.as_bytes();
        let name = if *bytes.last().unwrap() == 0 {
            &bytes[..bytes.len() - 1]
        } else {
            bytes
        };
        let symbol = unsafe { self.hashtab.find(name, self.symtab, &self.strtab) };
        if let Some(sym) = symbol {
            return Some(unsafe {
                self.segments
                    .as_mut_ptr()
                    .add(sym.st_value as usize - self.segments.addr_min())
                    as *const ()
            });
        }
        None
    }

    pub fn from_file(path: &Path) -> Result<ELFLibrary> {
        let file = ELFFile::from_file(path)?;
        Self::load_library(file)
    }

	pub fn from_binary(bytes:&[u8])->Result<ELFLibrary>{
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

        if unlikely(unwind_info.is_none()) {
            return elfloader_error("elf file does not have .eh_frame_hdr section");
        }

        #[cfg(feature = "unwinding")]
        if unlikely(text_start == usize::MAX || text_start == text_end) {
            return elfloader_error("can not find .text start");
        }

        let memory_slice = segments.as_mut_slice();
        let memory_ptr = segments.as_mut_ptr();
        let dynamics = dynamics.unwrap();
        let unwind_info = unwind_info.unwrap();

        let mut hash_off = usize::MAX;
        let mut symtab_off = usize::MAX;
        let mut strtab_off = usize::MAX;
        let mut strtab_size = usize::MAX;
        let mut pltrel_size = usize::MAX;
        let mut pltrel_off = usize::MAX;
        let mut is_rel = true;

        for dynamic in dynamics.iter() {
            match dynamic.d_tag {
                DT_GNU_HASH => hash_off = dynamic.d_un as usize,
                DT_SYMTAB => symtab_off = dynamic.d_un as usize,
                DT_STRTAB => strtab_off = dynamic.d_un as usize,
                DT_STRSZ => strtab_size = dynamic.d_un as usize,
                DT_PLTRELSZ => pltrel_size = dynamic.d_un as usize,
                DT_JMPREL => pltrel_off = dynamic.d_un as usize,
                DT_PLTREL => is_rel = dynamic.d_un == DT_REL as u64,
                _ => {}
            }
        }

        //检验elfloader需要使用的节是否都存在
        if unlikely(hash_off == usize::MAX) {
            return elfloader_error("dynamic section does not have DT_GNU_HASH");
        }

        if unlikely(symtab_off == usize::MAX) {
            return elfloader_error("dynamic section does not have DT_SYMTAB");
        }

        if unlikely(strtab_off == usize::MAX) {
            return elfloader_error("dynamic section does not have DT_STRTAB");
        }

        if unlikely(strtab_size == usize::MAX) {
            return elfloader_error("dynamic section does not have DT_STRSZ");
        }

        hash_off -= addr_min;
        symtab_off -= addr_min;
        strtab_off -= addr_min;

        let strtab_off_end = strtab_off + strtab_size;
        let strtab = elf::string_table::StringTable::new(&memory_slice[strtab_off..strtab_off_end]);

        //可能没有重定位段
        let rela_sections = if pltrel_off != usize::MAX {
            if unlikely(is_rel) {
                return elfloader_error("unsupport rel");
            }
            pltrel_off -= addr_min;
            ELFRelas {
                pltrel: unsafe {
                    core::slice::from_raw_parts(
                        segments.as_mut_ptr().add(pltrel_off) as _,
                        pltrel_size / core::mem::size_of::<Rela>(),
                    )
                },
            }
        } else {
            ELFRelas { pltrel: &[] }
        };

        let hashtab = unsafe { ELFGnuHash::parse(memory_ptr.add(hash_off)) };
        let symtab = unsafe { memory_ptr.add(symtab_off) as _ };

        unwind_info.register_unwind_info();

        let elf_lib = ELFLibrary {
            segments,
            hashtab,
            symtab,
            strtab,
            unwind_info,
            rela_sections,
            relro,
        };
        Ok(elf_lib)
    }
}
