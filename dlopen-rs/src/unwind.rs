use core::ffi::c_void;

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

impl Drop for ELFUnwind {
    #[cfg(feature = "unwinding")]
    fn drop(&mut self) {
        use eh_finder::EH_FINDER;
        use hashbrown::hash_table::Entry;
        let mut eh_finder = unsafe { EH_FINDER.eh_info.write() };
        if let Entry::Occupied(entry) = eh_finder.entry(
            self.eh_frame_hdr as u64,
            |val| val.eh_frame_hdr == self.eh_frame_hdr,
            ELFUnwind::hasher,
        ) {
            let info = entry.remove();
            core::mem::forget(info.0);
        } else {
            unreachable!();
        };
    }

    #[cfg(all(target_env = "gnu", not(feature = "unwinding")))]
    fn drop(&mut self) {
        extern "C" {
            fn __deregister_frame(begin: *const c_void);
        }
        unsafe { __deregister_frame(self.0 as _) };
    }

    #[cfg(all(not(target_env = "gnu"), not(feature = "unwinding")))]
    fn drop(&mut self) {
        extern "C" {
            fn __deregister_frame(begin: *const c_void);
        }

        let mut current = self.0;
        let mut len = unsafe { core::ptr::read::<u32>(current as *const u32) } as u64;
        current += 4;
        if len == 0xFFFFFFFF {
            len = unsafe { core::ptr::read::<u64>(current as *const u64) };
            current += 8;
        }

        //跳过CIE
        current += len as usize;

        loop {
            let fde = current;
            len = unsafe { core::ptr::read::<u32>(current as *const u32) } as u64;
            current += 4;
            if len == 0xFFFFFFFF {
                len = unsafe { core::ptr::read::<u64>(current as *const u64) };
                current += 8;
            }
            if len == 0 {
                break;
            }
            unsafe { __deregister_frame(fde as _) };
            current += len as usize;
        }
    }
}

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

    #[cfg(all(target_env = "gnu", not(feature = "unwinding")))]
    #[inline]
    pub(crate) fn register_unwind_info(&self) {
        extern "C" {
            fn __register_frame(begin: *const c_void);
        }
        //在使用libgcc的情况下直接传eh_frame的地址即可
        unsafe { __register_frame(self.0 as _) };
    }

    #[cfg(all(not(target_env = "gnu"), not(feature = "unwinding")))]
    #[inline]
    unsafe fn register_unwind_info(unwind_info: &ELFUnwind) {
        extern "C" {
            fn __register_frame(begin: *const c_void);
        }

        // 使用libunwind时__register_frame传入的参数只能是单个的fde
        let mut current = unwind_info.0;
        let mut len = core::ptr::read::<u32>(current as *const u32) as u64;
        current += 4;
        if len == 0xFFFFFFFF {
            len = core::ptr::read::<u64>(current as *const u64);
            current += 8;
        }

        //跳过CIE
        current += len as usize;

        loop {
            let fde = current;
            len = core::ptr::read::<u32>(current as *const u32) as u64;
            current += 4;
            if len == 0xFFFFFFFF {
                len = core::ptr::read::<u64>(current as *const u64);
                current += 8;
            }
            if len == 0 {
                break;
            }
            __register_frame(fde as _);
            current += len as usize;
        }
    }

    #[cfg(feature = "unwinding")]
    #[inline]
    unsafe fn register_unwind_info(unwind_info: &ELFUnwind) {
        use eh_finder::EH_FINDER;
        use hashbrown::hash_map::DefaultHashBuilder;

        EH_FINDER.eh_info.write().insert_unique(
            unwind_info.eh_frame_hdr as u64,
            unwind_info.clone(),
            ELFUnwind::hasher,
        );
    }

    #[cfg(feature = "unwinding")]
    //每个unwind_info的eh_frame_hdr都是不同的
    fn hasher(val: &ELFUnwind) -> u64 {
        val.eh_frame_hdr as u64
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
