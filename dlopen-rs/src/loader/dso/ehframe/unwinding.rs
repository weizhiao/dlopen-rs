use core::{ops::Range, sync::atomic::AtomicBool};

use crate::{
    loader::{arch::Phdr, dso::segment::ELFSegments},
    Result,
};

use hashbrown::{hash_table::Entry, HashTable};
use spin::RwLock;
use unwinding::custom_eh_frame_finder::{
    set_custom_eh_frame_finder, EhFrameFinder, FrameInfo, FrameInfoKind,
};

#[derive(Debug)]
pub(crate) struct ELFUnwind(usize);

impl ELFUnwind {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> Result<ELFUnwind> {
        let eh_frame_hdr = segments.base() + phdr.p_vaddr as usize;
        Ok(ELFUnwind(eh_frame_hdr))
    }
}

impl Drop for ELFUnwind {
    fn drop(&mut self) {
        let mut eh_finder = EH_FINDER.unwind_infos.write();
        if let Entry::Occupied(entry) = eh_finder.entry(
            self.0 as u64,
            |val| val.eh_frame_hdr == self.0,
            ELFUnwind::hasher,
        ) {
            let _ = entry.remove();
        } else {
            unreachable!();
        };
    }
}

static IS_SET: AtomicBool = AtomicBool::new(false);
static EH_FINDER: EhFinder = EhFinder::new();

impl ELFUnwind {
    #[inline]
    pub(crate) fn register_unwind(&self, segments: &ELFSegments) {
        if !IS_SET.swap(true, core::sync::atomic::Ordering::SeqCst) {
            set_custom_eh_frame_finder(&EH_FINDER).unwrap();
        }

        let unwind_info = UnwindInfo {
            eh_frame_hdr: self.0,
            pc_range: segments.base()..segments.base() + segments.len,
        };

        EH_FINDER
            .unwind_infos
            .write()
            .insert_unique(self.0 as u64, unwind_info, ELFUnwind::hasher);
    }

    //每个unwind_info的eh_frame_hdr都是不同的
    fn hasher(val: &UnwindInfo) -> u64 {
        val.eh_frame_hdr as u64
    }
}

struct UnwindInfo {
    eh_frame_hdr: usize,
    pc_range: Range<usize>,
}

struct EhFinder {
    unwind_infos: RwLock<HashTable<UnwindInfo>>,
}

impl EhFinder {
    const fn new() -> EhFinder {
        EhFinder {
            unwind_infos: RwLock::new(HashTable::new()),
        }
    }
}

unsafe impl EhFrameFinder for EhFinder {
    fn find(&self, pc: usize) -> Option<FrameInfo> {
        let unwind_infos = self.unwind_infos.read();
        for unwind_info in &*unwind_infos {
            let eh_frame_hdr = unwind_info.eh_frame_hdr;
            if unwind_info.pc_range.contains(&pc) {
                return Some(FrameInfo {
                    text_base: None,
                    kind: FrameInfoKind::EhFrameHdr(eh_frame_hdr),
                });
            }
        }
        None
    }
}
