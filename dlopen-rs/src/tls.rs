use std::{mem::MaybeUninit, num::NonZeroUsize};

use nix::{
    libc::{pthread_getspecific, pthread_key_create, pthread_key_t, pthread_setspecific},
    sys::mman::{self, MapFlags, ProtFlags},
};

use crate::{segment::ELFSegments, Phdr};

#[derive(Debug)]
pub(crate) struct ELFTLS {
    align: usize,
    image: *const u8,
    size: NonZeroUsize,
    key: pthread_key_t,
}

impl ELFTLS {
    pub(crate) unsafe fn new(phdr: &Phdr, segments: &ELFSegments) -> ELFTLS {
        let mut key = MaybeUninit::uninit();
        // FIXME:释放内存
        pthread_key_create(key.as_mut_ptr(), None);

        ELFTLS {
            align: phdr.p_align as usize,
            image: segments.as_mut_ptr().add(phdr.p_vaddr as usize),
            size: NonZeroUsize::new(phdr.p_memsz as usize).unwrap(),
            key: key.assume_init(),
        }
    }
}

#[repr(C)]
pub(crate) struct TLSArg<'a> {
    tls: &'a ELFTLS,
    offset: usize,
}

pub(crate) unsafe extern "C" fn tls_get_addr(args: &TLSArg) -> *const u8 {
    let val = pthread_getspecific(args.tls.key);
    let memory = if val.is_null() {
        let memory: *mut u8 = mman::mmap_anonymous(
            None,
            args.tls.size,
            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
            MapFlags::MAP_PRIVATE,
        )
        .unwrap()
        .as_ptr()
        .cast();
        memory.copy_from_nonoverlapping(args.tls.image, args.tls.size.get());
        pthread_setspecific(args.tls.key, memory.cast());
        memory
    } else {
        val as *mut u8
    };

    memory.add(args.offset)
}
