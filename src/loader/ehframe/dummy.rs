use core::ops::Range;

#[derive(Debug, Clone)]
pub(crate) struct EhFrame;

impl EhFrame {
    pub(crate) fn new(_phdr: &elf_loader::arch::Phdr, _map_range: Range<usize>) -> Option<Self> {
        None
    }
}
