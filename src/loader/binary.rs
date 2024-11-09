use super::{
    mmap::{Offset, RawData},
    types::ELFEhdr,
    MapSegment, SharedObject, MASK,
};
use alloc::vec::Vec;
use core::ops::Range;

pub(crate) struct ELFBinary<'a> {
    bytes: &'a [u8],
}

impl<'bytes> ELFBinary<'bytes> {
    pub(crate) const fn new(bytes: &'bytes [u8]) -> Self {
        ELFBinary { bytes }
    }
}

impl RawData for ELFBinary<'_> {
    fn transport(&self, offset: usize, len: usize) -> super::mmap::Offset {
        Offset {
            kind: super::mmap::OffsetType::Addr(unsafe { self.bytes.as_ptr().add(offset) }),
            offset: offset - (offset & MASK),
            len,
        }
    }
}

impl MapSegment for ELFBinary<'_> {}

impl<'bytes> SharedObject for ELFBinary<'bytes> {
    fn parse_ehdr(&mut self) -> crate::Result<(Range<usize>, Vec<u8>)> {
        let ehdr = ELFEhdr::new(self.bytes)?;
        ehdr.validate()?;

        let (phdr_start, phdr_end) = ehdr.phdr_range();
        let phdrs = &self.bytes[phdr_start..phdr_end];
        Ok((phdr_start..phdr_end, phdrs.to_vec()))
    }
}
