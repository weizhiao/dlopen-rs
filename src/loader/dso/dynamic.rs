use core::slice::from_raw_parts;

use crate::{
    loader::arch::{Dyn, Phdr, Rela},
    parse_dynamic_error, Result,
};
use alloc::vec::Vec;
use elf::abi::*;

use super::segment::ELFSegments;

pub(crate) struct ELFDynamic {
    #[cfg(feature = "debug")]
    dyn_addr: usize,
    hash_off: usize,
    symtab_off: usize,
    strtab: &'static [u8],
    init_fn: Option<extern "C" fn()>,
    init_array_fn: Option<&'static [extern "C" fn()]>,
    fini_fn: Option<extern "C" fn()>,
    fini_array_fn: Option<&'static [extern "C" fn()]>,
    pltrel: Option<&'static [Rela]>,
    rela: Option<&'static [Rela]>,
    needed_libs: Vec<usize>,
}

impl ELFDynamic {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> Result<ELFDynamic> {
        let dynamic_start = phdr.p_vaddr as usize;
        let dynamic_len = phdr.p_memsz as usize / size_of::<Dyn>();
        let dynamic_addr = segments.base() + dynamic_start;
        let dynamics = unsafe { from_raw_parts(dynamic_addr as *const Dyn, dynamic_len) };

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
        let mut needed_libs = Vec::new();

        for dynamic in dynamics {
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
                _ => {}
            }
        }

        let hash_off = hash_off.map(|val| val + base).ok_or(parse_dynamic_error(
            "dynamic section does not have DT_GNU_HASH",
        ))?;

        let symtab_off = symtab_off.map(|val| val + base).ok_or(parse_dynamic_error(
            "dynamic section does not have DT_SYMTAB",
        ))?;

        let strtab = strtab_off
            .map(|strtab_off| {
                &segments.as_mut_slice()[strtab_off..strtab_off + strtab_size.unwrap()]
            })
            .ok_or(parse_dynamic_error(
                "dynamic section does not have DT_STRTAB",
            ))?;

        let pltrel = pltrel_off.map(|pltrel_off| unsafe {
            from_raw_parts(
                segments.as_mut_ptr().add(pltrel_off) as _,
                pltrel_size.unwrap() / size_of::<Rela>(),
            )
        });

        let rela = rela_off.map(|rel_off| unsafe {
            from_raw_parts(
                segments.as_mut_ptr().add(rel_off) as _,
                rela_size.unwrap() / size_of::<Rela>(),
            )
        });

        let init_fn = init_off.map(|val| unsafe { core::mem::transmute(val + base) });

        let init_array_fn = init_array_off.map(|init_array_off| {
            let ptr = init_array_off + base;
            unsafe { from_raw_parts(ptr as _, init_array_size.unwrap() / size_of::<usize>()) }
        });

        let fini_array_fn = fini_array_off.map(|fini_array_off| {
            let ptr = fini_array_off + base;
            unsafe { from_raw_parts(ptr as _, fini_array_size.unwrap() / size_of::<usize>()) }
        });

        let fini_fn = fini_off.map(|fini_off| unsafe { core::mem::transmute(fini_off + base) });

        Ok(ELFDynamic {
            #[cfg(feature = "debug")]
            dyn_addr: dynamic_addr,
            hash_off,
            symtab_off,
            strtab,
            init_fn,
            init_array_fn,
            fini_fn,
            fini_array_fn,
            pltrel,
            rela,
            needed_libs,
        })
    }

    #[cfg(feature = "debug")]
    pub(crate) fn addr(&self) -> usize {
        self.dyn_addr
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

    pub(crate) fn needed_libs(&self) -> &Vec<usize> {
        &self.needed_libs
    }
}
