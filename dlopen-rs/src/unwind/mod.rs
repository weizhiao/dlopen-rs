#[cfg(feature="libgcc")]
mod libgcc;
#[cfg(feature="libunwind")]
mod libunwind;
#[cfg(feature="unwinding")]
mod unwinding;

use crate::{gimli_err_convert, segment::ELFSegments, Phdr, Result};

#[cfg(feature = "unwinding")]
#[derive(Debug, Clone)]
pub struct UnwindInfo {
    pub eh_frame_hdr: usize,
    pub text_start: usize,
    pub text_end: usize,
}

#[cfg(not(feature = "unwinding"))]
#[derive(Debug)]
pub(crate) struct ELFUnwind(usize);


impl ELFUnwind {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> Result<ELFUnwind> {
        let base = segments.base();
        let eh_frame_hdr_off = phdr.p_vaddr as usize;
        let eh_frame_hdr_size = phdr.p_memsz as usize;
        let bases =
            gimli::BaseAddresses::default().set_eh_frame_hdr((eh_frame_hdr_off + base) as _);
        let eh_frame_hdr = gimli::EhFrameHdr::new(
            &segments.as_mut_slice()[eh_frame_hdr_off..eh_frame_hdr_off + eh_frame_hdr_size],
            gimli::NativeEndian,
        )
        .parse(&bases, core::mem::size_of::<usize>() as _)
        .map_err(gimli_err_convert)?;
        let eh_frame_addr = match eh_frame_hdr.eh_frame_ptr() {
            gimli::Pointer::Direct(x) => x as usize,
            gimli::Pointer::Indirect(x) => unsafe { *(x as *const _) },
        };
        Ok(ELFUnwind(eh_frame_addr))
    }
}

#[cfg(feature = "unwinding")]
pub(crate) mod eh_finder {

    use super::{ELFLibraryInner, UnwindInfo};
    use alloc::sync::Arc;
    use hashbrown::HashTable;
    use spin::RwLock;
    use unwinding::custom_eh_frame_finder::{EhFrameFinder, FrameInfo, FrameInfoKind};

    pub(crate) static mut EH_FINDER: EhFinder = EhFinder::new();

    pub(crate) struct EhFinder {
        pub eh_info: RwLock<HashTable<ELFUnwind>>,
    }

    impl EhFinder {
        const fn new() -> EhFinder {
            EhFinder {
                eh_info: RwLock::new(HashTable::new()),
            }
        }
    }

    unsafe impl EhFrameFinder for EhFinder {
        fn find(&self, pc: usize) -> Option<FrameInfo> {
            let unwind_info = self.eh_info.read();
            for eh_frame_info in &*unwind_info {
                let text_start = eh_frame_info.text_start;
                let text_end = eh_frame_info.text_end;
                let eh_frame_hdr = eh_frame_info.eh_frame_hdr;
                if (text_start..text_end).contains(&pc) {
                    return Some(FrameInfo {
                        text_base: Some(text_start),
                        kind: FrameInfoKind::EhFrameHdr(eh_frame_hdr),
                    });
                }
            }
            None
        }
    }
}
