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
}

impl Iterator for ELFDynamic {
    type Item = &'static Dyn;

    fn next(&mut self) -> Option<Self::Item> {
        self.dynamic.iter().next()
    }
}
