use core::alloc::Layout;

use alloc::alloc::dealloc;

use crate::Result;

use super::{ELFRelro, ELFSegments, ALIGN};

impl Drop for ELFSegments {
    fn drop(&mut self) {
        unsafe {
            dealloc(
                self.memory.as_ptr() as _,
                Layout::from_size_align_unchecked(self.len, ALIGN),
            )
        }
    }
}

impl ELFRelro {
    #[inline]
    pub(crate) fn relro(&self) -> Result<()> {
        Ok(())
    }
}
