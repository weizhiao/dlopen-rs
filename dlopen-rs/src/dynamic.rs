use crate::{elfloader_error, segment::ELFSegments, Dyn, Phdr, Rela, Result};
use elf::abi::*;

pub(crate) struct ELFDynamic {
    hash_off: usize,
    symtab_off: usize,
    strtab: &'static [u8],
    init_fn: Option<extern "C" fn()>,
    init_array_fn: Option<&'static [extern "C" fn()]>,
    fini_fn: Option<extern "C" fn()>,
    fini_array_fn: Option<&'static [extern "C" fn()]>,
    pltrel: Option<&'static [Rela]>,
    rela: Option<&'static [Rela]>,
}

impl ELFDynamic {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> Result<ELFDynamic> {
        let dynamic_start = phdr.p_vaddr as usize;
        let dynamic_len = phdr.p_memsz as usize / core::mem::size_of::<Dyn>();
        let dynamics = unsafe {
            core::slice::from_raw_parts(
                (segments.base() + dynamic_start) as *const Dyn,
                dynamic_len,
            )
        };

        let base = segments.base();

        let mut hash_off = None;
        let mut symtab_off = None;
        let mut strtab_off = None;
        let mut strtab_size = None;
        let mut pltrel_size = None;
        let mut pltrel_off = None;
        let mut rela_off = None;
        let mut rela_size = None;
        let mut init_off = None;
        let mut fini_off = None;
        let mut init_array_off = None;
        let mut init_array_size = None;
        let mut fini_array_off = None;
        let mut fini_array_size = None;

        for dynamic in dynamics {
            match dynamic.d_tag {
                DT_GNU_HASH => hash_off = Some(dynamic.d_un as usize),
                DT_SYMTAB => symtab_off = Some(dynamic.d_un as usize),
                DT_STRTAB => strtab_off = Some(dynamic.d_un as usize),
                DT_STRSZ => strtab_size = Some(dynamic.d_un as usize),
                DT_PLTRELSZ => pltrel_size = Some(dynamic.d_un as usize),
                DT_JMPREL => pltrel_off = Some(dynamic.d_un as usize),
                DT_RELA => rela_off = Some(dynamic.d_un as usize),
                DT_RELASZ => rela_size = Some(dynamic.d_un as usize),
                DT_INIT => init_off = Some(dynamic.d_un as usize),
                DT_FINI => fini_off = Some(dynamic.d_un as usize),
                DT_INIT_ARRAY => init_array_off = Some(dynamic.d_un as usize),
                DT_INIT_ARRAYSZ => init_array_size = Some(dynamic.d_un as usize),
                DT_FINI_ARRAY => fini_array_off = Some(dynamic.d_un as usize),
                DT_FINI_ARRAYSZ => fini_array_size = Some(dynamic.d_un as usize),
                _ => {}
            }
        }

        let hash_off = if let Some(hash_off) = hash_off {
            hash_off + base
        } else {
            return elfloader_error("dynamic section does not have DT_GNU_HASH");
        };

        let symtab_off = if let Some(symtab_off) = symtab_off {
            symtab_off + base
        } else {
            return elfloader_error("dynamic section does not have DT_SYMTAB");
        };

        let strtab = if let Some(strtab_off) = strtab_off {
            &segments.as_mut_slice()[strtab_off..strtab_off + strtab_size.unwrap()]
        } else {
            return elfloader_error("dynamic section does not have DT_STRTAB");
        };

        let pltrel = if let Some(pltrel_off) = pltrel_off {
            Some(unsafe {
                core::slice::from_raw_parts(
                    segments.as_mut_ptr().add(pltrel_off) as _,
                    pltrel_size.unwrap() / core::mem::size_of::<Rela>(),
                )
            })
        } else {
            None
        };

        let rela = if let Some(rel_off) = rela_off {
            Some(unsafe {
                core::slice::from_raw_parts(
                    segments.as_mut_ptr().add(rel_off) as _,
                    rela_size.unwrap() / core::mem::size_of::<Rela>(),
                )
            })
        } else {
            None
        };

        let init_fn: Option<extern "C" fn()> = if let Some(init_off) = init_off {
            unsafe { core::mem::transmute(init_off + base) }
        } else {
            None
        };

        let init_array_fn: Option<&'static [extern "C" fn()]> =
            if let Some(init_array_off) = init_array_off {
                let ptr = init_array_off + base;
                unsafe {
                    Some(core::slice::from_raw_parts(
                        ptr as _,
                        init_array_size.unwrap() / core::mem::size_of::<usize>(),
                    ))
                }
            } else {
                None
            };

        let fini_array_fn: Option<&'static [extern "C" fn()]> =
            if let Some(fini_array_off) = fini_array_off {
                let ptr = fini_array_off + base;
                unsafe {
                    Some(core::slice::from_raw_parts(
                        ptr as _,
                        fini_array_size.unwrap() / core::mem::size_of::<usize>(),
                    ))
                }
            } else {
                None
            };

        let fini_fn: Option<extern "C" fn()> = if let Some(fini_off) = fini_off {
            unsafe { core::mem::transmute(fini_off + base) }
        } else {
            None
        };

        Ok(ELFDynamic {
            hash_off,
            symtab_off,
            strtab,
            init_fn,
            init_array_fn,
            fini_fn,
            fini_array_fn,
            pltrel,
            rela,
        })
    }

    pub(crate) fn strtab(&self) -> &'static [u8] {
        self.strtab
    }

    pub(crate) fn hash(&self) -> usize {
        self.hash_off
    }

    pub(crate) fn dynsym(&self) -> usize {
        self.symtab_off
    }

    pub(crate) fn pltrel(&self) -> Option<&'static [Rela]> {
        self.pltrel
    }

    pub(crate) fn rela(&self) -> Option<&'static [Rela]> {
        self.rela
    }

    pub(crate) fn init_fn(&self) -> Option<extern "C" fn()> {
        self.init_fn
    }

    pub(crate) fn init_array_fn(&self) -> Option<&'static [extern "C" fn()]> {
        self.init_array_fn
    }

    pub(crate) fn fini_fn(&self) -> Option<extern "C" fn()> {
        self.fini_fn
    }

    pub(crate) fn fini_array_fn(&self) -> Option<&'static [extern "C" fn()]> {
        self.fini_array_fn
    }
}
