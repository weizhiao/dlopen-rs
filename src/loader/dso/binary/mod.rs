use alloc::vec::Vec;

use super::{ehdr::ELFEhdr, SharedObject};

#[cfg(feature = "mmap")]
mod mmap;
#[cfg(not(feature = "mmap"))]
mod no_mmap;

pub(crate) struct ELFBinary<'a> {
    bytes: &'a [u8],
}

impl ELFBinary<'_> {
    pub(crate) fn new(bytes: &[u8]) -> ELFBinary {
        ELFBinary { bytes }
    }
}

impl SharedObject for ELFBinary<'_> {
    fn parse_ehdr(&mut self) -> crate::Result<Vec<u8>> {
        let ehdr = ELFEhdr::new(self.bytes)?;
        ehdr.validate()?;

        let (phdr_start, phdr_end) = ehdr.phdr_range();
        let phdrs = &self.bytes[phdr_start..phdr_end];
        Ok(phdrs.to_vec())
    }
}
