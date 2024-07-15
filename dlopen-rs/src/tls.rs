use std::{cell::OnceCell, num::NonZeroUsize, ptr::NonNull, thread};

use nix::sys::mman::{self, MapFlags, ProtFlags};

use crate::{segment::ELFSegments, Phdr};

thread_local! {
    static TLS_MEMORY:OnceCell<NonNull<u8>>=OnceCell::new();
}

#[derive(Debug)]
pub(crate) struct ELFTLS {
    align: usize,
    image: *const u8,
    size: NonZeroUsize,
}

impl ELFTLS {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> ELFTLS {
        ELFTLS {
            align: phdr.p_align as usize,
            image: unsafe { segments.as_mut_ptr().add(phdr.p_vaddr as usize) },
            size: NonZeroUsize::new(phdr.p_memsz as usize).unwrap(),
        }
    }
}

#[repr(C)]
struct TLSArg<'a> {
    tls: &'a ELFTLS,
    offset: usize,
}

extern "C" fn tls_get_addr(args: &TLSArg) -> *const u8 {
    let memory = TLS_MEMORY.with(|memory| {
        let memory = memory.get_or_init(|| unsafe {
            mman::mmap_anonymous(
                None,
                args.tls.size,
                ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
                MapFlags::MAP_PRIVATE,
            )
            .unwrap()
            .cast()
        });
        memory.as_ptr()
    });

    unsafe { memory.add(args.offset) }
}
