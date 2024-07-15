use std::{alloc::Layout, mem::MaybeUninit, sync::Mutex};

use nix::libc::{
    pthread_getspecific, pthread_key_create, pthread_key_delete, pthread_key_t, pthread_setspecific,
};

use crate::{segment::ELFSegments, Phdr};

#[derive(Debug)]
pub(crate) struct ELFTLS {
    align: usize,
    image: *const u8,
    len: usize,
    size: usize,
    key: pthread_key_t,
	/// 用于释放内存
    mem_pool: Mutex<Vec<(*mut u8, Layout)>>,
}

impl ELFTLS {
    pub(crate) unsafe fn new(phdr: &Phdr, segments: &ELFSegments) -> ELFTLS {
        let mut key = MaybeUninit::uninit();
        // FIXME:释放内存
        pthread_key_create(key.as_mut_ptr(), None);

        ELFTLS {
            align: phdr.p_align as usize,
            image: segments.as_mut_ptr().add(phdr.p_vaddr as usize),
            len: phdr.p_filesz as usize,
            size: phdr.p_memsz as usize,
            key: key.assume_init(),
            mem_pool: Mutex::new(Vec::new()),
        }
    }
}

impl Drop for ELFTLS {
    fn drop(&mut self) {
        unsafe { pthread_key_delete(self.key) };
        for (ptr, layout) in self.mem_pool.get_mut().unwrap().iter() {
            unsafe { alloc::alloc::dealloc(*ptr, *layout) }
        }
    }
}

#[repr(C)]
pub(crate) struct TLSArg<'a> {
    tls: &'a ELFTLS,
}

pub(crate) unsafe extern "C" fn tls_get_addr(args: &TLSArg) -> *const u8 {
    let val = pthread_getspecific(args.tls.key);
    let memory = if val.is_null() {
        let layout = Layout::from_size_align(args.tls.size, args.tls.align).unwrap();
        let memory = alloc::alloc::alloc_zeroed(layout);
        memory.copy_from_nonoverlapping(args.tls.image, args.tls.len);
        pthread_setspecific(args.tls.key, memory.cast());
        args.tls.mem_pool.lock().unwrap().push((memory, layout));
        memory
    } else {
        val as *mut u8
    };

    memory
}
