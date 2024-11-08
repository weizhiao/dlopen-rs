use crate::{
    loader::{segment::ELFSegments, Phdr},
    Result,
};

#[derive(Debug)]
pub(crate) struct EhFrame;

impl EhFrame {
    pub(crate) fn new(_phdr: &Phdr, _segments: &ELFSegments) -> Result<EhFrame> {
        Ok(EhFrame)
    }
}