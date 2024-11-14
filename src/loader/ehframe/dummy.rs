use core::ops::Range;
use elf_loader::Unwind;

#[derive(Debug, Clone)]
pub(crate) struct EhFrame;

impl Unwind for EhFrame {
    unsafe fn new(_phdr: &elf_loader::arch::Phdr, _map_range: Range<usize>) -> Option<Self> {
        None
    }
}
