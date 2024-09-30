use super::{hashtab::ELFGnuHash, ELFStringTable};
use crate::{
    loader::arch::{Dyn, ELFSymbol, Rela},
    parse_dynamic_error, Result,
};
use alloc::vec::Vec;
use core::{slice::from_raw_parts, usize};
use elf::abi::*;

pub(crate) struct ELFRawDynamic {
    #[cfg(feature = "debug")]
    dyn_addr: usize,
    hash_off: usize,
    symtab_off: usize,
    strtab_off: usize,
    strtab_size: usize,
    pltrel_off: Option<usize>,
    pltrel_size: Option<usize>,
    rela_off: Option<usize>,
    rela_size: Option<usize>,
    init_off: Option<usize>,
    fini_off: Option<usize>,
    init_array_off: Option<usize>,
    init_array_size: Option<usize>,
    fini_array_off: Option<usize>,
    fini_array_size: Option<usize>,
    version_ids_off: Option<usize>,
    verneed_off: Option<usize>,
    verneed_num: Option<usize>,
    verdef_off: Option<usize>,
    verdef_num: Option<usize>,
    needed_libs: Vec<usize>,
}

impl ELFRawDynamic {
    pub(crate) fn new(dynamic_ptr: *const Dyn) -> Result<ELFRawDynamic> {
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
        let mut version_ids_off = None;
        let mut verneed_off = None;
        let mut verneed_num = None;
        let mut verdef_off = None;
        let mut verdef_num = None;
        let mut needed_libs = Vec::new();

        let mut cur_dyn_ptr = dynamic_ptr;
        let mut dynamic = unsafe { &*cur_dyn_ptr };

        loop {
            match dynamic.d_tag {
                DT_NEEDED => needed_libs.push(dynamic.d_un as usize),
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
                DT_VERSYM => version_ids_off = Some(dynamic.d_un as usize),
                DT_VERNEED => verneed_off = Some(dynamic.d_un as usize),
                DT_VERNEEDNUM => verneed_num = Some(dynamic.d_un as usize),
                DT_VERDEF => verdef_off = Some(dynamic.d_un as usize),
                DT_VERDEFNUM => verdef_num = Some(dynamic.d_un as usize),
                DT_NULL => break,
                _ => {}
            }
            cur_dyn_ptr = unsafe { cur_dyn_ptr.add(1) };
            dynamic = unsafe { &*cur_dyn_ptr };
        }

        let hash_off = hash_off.ok_or(parse_dynamic_error(
            "dynamic section does not have DT_GNU_HASH",
        ))?;
        let symtab_off = symtab_off.ok_or(parse_dynamic_error(
            "dynamic section does not have DT_SYMTAB",
        ))?;
        let strtab_off = strtab_off.ok_or(parse_dynamic_error(
            "dynamic section does not have DT_STRTAB",
        ))?;
        let strtab_size = strtab_size.ok_or(parse_dynamic_error(
            "dynamic section does not have DT_STRSZ",
        ))?;
        Ok(ELFRawDynamic {
            #[cfg(feature = "debug")]
            dyn_addr: dynamic_ptr as usize,
            hash_off,
            symtab_off,
            needed_libs,
            strtab_off,
            strtab_size,
            pltrel_off,
            pltrel_size,
            rela_off,
            rela_size,
            init_off,
            fini_off,
            init_array_off,
            init_array_size,
            fini_array_off,
            fini_array_size,
            version_ids_off,
            verneed_off,
            verneed_num,
            verdef_off,
            verdef_num,
        })
    }

    pub(crate) fn finish(self, base: usize) -> ELFDynamic {
        let hashtable = unsafe { ELFGnuHash::parse((self.hash_off + base) as *const u8) };
        let symtab = (self.symtab_off + base) as *const ELFSymbol;
        let strtab = ELFStringTable::new(unsafe {
            from_raw_parts((base + self.strtab_off) as *const u8, self.strtab_size)
        });
        let pltrel = self.pltrel_off.map(|pltrel_off| unsafe {
            from_raw_parts(
                (base + pltrel_off) as *const Rela,
                self.pltrel_size.unwrap() / size_of::<Rela>(),
            )
        });
        let rela = self.rela_off.map(|rel_off| unsafe {
            from_raw_parts(
                (base + rel_off) as *const Rela,
                self.rela_size.unwrap() / size_of::<Rela>(),
            )
        });
        let init_fn = self
            .init_off
            .map(|val| unsafe { core::mem::transmute(val + base) });
        let init_array_fn = self.init_array_off.map(|init_array_off| {
            let ptr = init_array_off + base;
            unsafe { from_raw_parts(ptr as _, self.init_array_size.unwrap() / size_of::<usize>()) }
        });
        let fini_fn = self
            .fini_off
            .map(|fini_off| unsafe { core::mem::transmute(fini_off + base) });
        let fini_array_fn = self.fini_array_off.map(|fini_array_off| {
            let ptr = fini_array_off + base;
            unsafe { from_raw_parts(ptr as _, self.fini_array_size.unwrap() / size_of::<usize>()) }
        });
        let needed_libs: Vec<&'static str> = self
            .needed_libs
            .iter()
            .map(|needed_lib| strtab.get(*needed_lib))
            .collect();
        let verneed = self
            .verneed_off
            .map(|verneed_off| (verneed_off, self.verneed_num.unwrap()));
        let verdef = self
            .verdef_off
            .map(|verdef_off| (verdef_off, self.verdef_num.unwrap()));
        ELFDynamic {
            #[cfg(feature = "debug")]
            dyn_addr: self.dyn_addr,
            hashtab: hashtable,
            symtab,
            strtab,
            init_fn,
            init_array_fn,
            fini_fn,
            fini_array_fn,
            pltrel,
            rela,
            needed_libs,
            version_idx: self.version_ids_off,
            verneed,
            verdef,
        }
    }

    #[cfg(feature = "ldso")]
    pub(crate) fn hash_off(&self) -> usize {
        self.hash_off
    }
}

#[allow(unused)]
pub(crate) struct ELFDynamic {
    #[cfg(feature = "debug")]
    dyn_addr: usize,
    hashtab: ELFGnuHash,
    symtab: *const ELFSymbol,
    strtab: ELFStringTable<'static>,
    init_fn: Option<extern "C" fn()>,
    init_array_fn: Option<&'static [extern "C" fn()]>,
    fini_fn: Option<extern "C" fn()>,
    fini_array_fn: Option<&'static [extern "C" fn()]>,
    pltrel: Option<&'static [Rela]>,
    rela: Option<&'static [Rela]>,
    needed_libs: Vec<&'static str>,
    version_idx: Option<usize>,
    verneed: Option<(usize, usize)>,
    verdef: Option<(usize, usize)>,
}

impl ELFDynamic {
    #[cfg(feature = "debug")]
    pub(crate) fn addr(&self) -> usize {
        self.dyn_addr
    }

    pub(crate) fn strtab(&self) -> ELFStringTable<'static> {
        self.strtab.clone()
    }

    pub(crate) fn hashtab(&self) -> ELFGnuHash {
        self.hashtab.clone()
    }

    pub(crate) fn symtab(&self) -> *const ELFSymbol {
        self.symtab
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

    #[cfg(feature = "version")]
    pub(crate) fn version_idx(&self) -> Option<usize> {
        self.version_idx
    }

    #[cfg(feature = "version")]
    pub(crate) fn verneed(&self) -> Option<(usize, usize)> {
        self.verneed
    }

    #[cfg(feature = "version")]
    pub(crate) fn verdef(&self) -> Option<(usize, usize)> {
        self.verdef
    }

    pub(crate) fn needed_libs(self) -> Vec<&'static str> {
        self.needed_libs
    }
}
