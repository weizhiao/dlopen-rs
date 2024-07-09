use std::slice::Iter;

use crate::{segment::ELFSegments, Dyn, Phdr};

pub(crate) struct ELFDynamic {
    dynamic: &'static [Dyn],
}

impl ELFDynamic {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> ELFDynamic {
        let dynamic_start = phdr.p_vaddr as usize - segments.addr_min();
        let dynamic_len = phdr.p_memsz as usize / core::mem::size_of::<Dyn>();
        let dynamic = unsafe {
            core::slice::from_raw_parts(
                segments.as_mut_ptr().add(dynamic_start) as *const Dyn,
                dynamic_len,
            )
        };
        ELFDynamic { dynamic }
    }

    pub(crate) fn iter(&self) -> Iter<'_, Dyn> {
        self.dynamic.iter()
    }
}
