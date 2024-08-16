use crate::{
    loader::{arch::Phdr, dso::segment::ELFSegments},
    Result,
};

#[derive(Debug)]
pub(crate) struct ELFUnwind;

impl ELFUnwind {
    pub(crate) fn new(_phdr: &Phdr, _segments: &ELFSegments) -> Result<ELFUnwind> {
        Ok(ELFUnwind)
    }
}

impl ELFUnwind {
    #[inline]
    pub(crate) fn register_unwind(&self, _segments: &ELFSegments) {}
}
