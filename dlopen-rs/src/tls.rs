use std::{
    alloc::{dealloc, Layout},
    mem::{size_of, MaybeUninit},
    os::raw::c_void,
};

use nix::libc::{
    pthread_getspecific, pthread_key_create, pthread_key_delete, pthread_key_t, pthread_setspecific,
};

use crate::{
    arch::{TLSIndex, TLS_DTV_OFFSET},
    segment::ELFSegments,
    Phdr,
};

#[derive(Debug)]
pub(crate) struct ELFTLS {
    image: *const u8,
    len: usize,
    layout: Layout,
    offset: usize,
    key: pthread_key_t,
}

impl ELFTLS {
    pub(crate) unsafe fn new(phdr: &Phdr, segments: &ELFSegments) -> ELFTLS {
        unsafe extern "C" fn dtor(ptr: *mut c_void) {
            if !ptr.is_null() {
                let layout = ptr.cast::<Layout>().read();
                dealloc(ptr.cast(), layout);
            }
        }

        let mut key = MaybeUninit::uninit();

        pthread_key_create(key.as_mut_ptr(), Some(dtor));

        let align = phdr.p_align as usize;
        let mut size = (size_of::<Layout>() + align - 1) & (-(align as isize) as usize);
        // 前面用来保存layout
        let offset = size;
        size += phdr.p_memsz as usize;
        let layout = Layout::from_size_align_unchecked(size, align);

        ELFTLS {
            image: segments.as_mut_ptr().add(phdr.p_vaddr as usize),
            len: phdr.p_filesz as usize,
            key: key.assume_init(),
            layout,
            offset,
        }
    }
}

impl Drop for ELFTLS {
    fn drop(&mut self) {
        unsafe { pthread_key_delete(self.key) };
    }
}

pub(crate) unsafe extern "C" fn tls_get_addr(tls_index: &TLSIndex) -> *const u8 {
    let tls = &*(tls_index.ti_module as *const ELFTLS);
    let val = pthread_getspecific(tls.key);
    let data = if val.is_null() {
        let layout = tls.layout;
        let memory = alloc::alloc::alloc_zeroed(layout);
        memory.cast::<Layout>().write(layout);
        let data = memory.add(tls.offset);
        data.copy_from_nonoverlapping(tls.image, tls.len);
        pthread_setspecific(tls.key, memory.cast());
        data
    } else {
        val.add(tls.offset).cast()
    };

    data.add(tls_index.ti_offset.wrapping_add(TLS_DTV_OFFSET))
}
