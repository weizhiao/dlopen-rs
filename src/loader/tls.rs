#[cfg(feature = "tls")]
mod imp {
    use elf_loader::{
        arch::{Phdr, TLS_DTV_OFFSET},
        ThreadLocal,
    };

    use libc::{
        pthread_getspecific, pthread_key_create, pthread_key_delete, pthread_key_t,
        pthread_setspecific,
    };
    use std::{
        alloc::{dealloc, handle_alloc_error, Layout},
        mem::{size_of, MaybeUninit},
        os::raw::c_void,
    };

    #[repr(C)]
    pub(crate) struct TlsIndex {
        ti_module: usize,
        ti_offset: usize,
    }

    struct TlsInner {
        image: *const u8,
        len: usize,
        layout: Layout,
        offset: usize,
        key: pthread_key_t,
    }

    pub(crate) struct ElfTls {
        inner: Box<TlsInner>,
    }

    const MAX_TLS_INDEX: usize = 4096;

    impl ThreadLocal for ElfTls {
        unsafe fn new(phdr: &Phdr, base: usize) -> Option<Self> {
            unsafe extern "C" fn dtor(ptr: *mut c_void) {
                if !ptr.is_null() {
                    let layout = ptr.cast::<Layout>().read();
                    dealloc(ptr.cast(), layout);
                }
            }

            let mut key = MaybeUninit::uninit();
            if pthread_key_create(key.as_mut_ptr(), Some(dtor)) != 0 {
                panic!("can not create tls");
            }
            let align = phdr.p_align as usize;
            let mut size = (size_of::<Layout>() + align - 1) & !(align - 1);
            // 前面用来保存layout
            let offset = size;
            size += phdr.p_memsz as usize;
            let layout = unsafe { Layout::from_size_align_unchecked(size, align) };
            let inner = Box::new(TlsInner {
                image: (base as *const u8).add(phdr.p_vaddr as usize),
                len: phdr.p_filesz as usize,
                key: key.assume_init(),
                layout,
                offset,
            });
            assert!((inner.as_ref() as *const _ as usize) > MAX_TLS_INDEX);
            Some(Self { inner })
        }

        unsafe fn module_id(&self) -> usize {
            self.inner.as_ref() as *const TlsInner as usize
        }
    }

    impl Drop for ElfTls {
        fn drop(&mut self) {
            unsafe { pthread_key_delete(self.inner.key) };
        }
    }

    pub(crate) unsafe extern "C" fn tls_get_addr(tls_index: &TlsIndex) -> *const u8 {
        if tls_index.ti_module > MAX_TLS_INDEX {
            let tls = &*(tls_index.ti_module as *const TlsInner);
            let val = pthread_getspecific(tls.key);
            let data = if val.is_null() {
                let layout = tls.layout;
                let memory = alloc::alloc::alloc_zeroed(layout);
                if memory.is_null() {
                    handle_alloc_error(layout);
                }
                memory.cast::<Layout>().write(layout);
                let data = memory.add(tls.offset);
                data.copy_from_nonoverlapping(tls.image, tls.len);
                if pthread_setspecific(tls.key, memory.cast()) != 0 {
                    return core::ptr::null();
                }
                data
            } else {
                val.add(tls.offset).cast()
            };
            data.add(tls_index.ti_offset.wrapping_add(TLS_DTV_OFFSET))
        } else {
            extern "C" {
                fn __tls_get_addr(tls_index: &TlsIndex) -> *const u8;
            }
            __tls_get_addr(tls_index)
        }
    }
}

#[cfg(not(feature = "tls"))]
mod imp {
    use elf_loader::ThreadLocal;

    #[derive(Clone)]
    pub(crate) struct ElfTls;
    impl ThreadLocal for ElfTls {
        unsafe fn new(_phdr: &elf_loader::arch::Phdr, _base: usize) -> Option<Self> {
            None
        }

        unsafe fn module_id(&self) -> usize {
            0
        }
    }

    pub(crate) fn tls_get_addr() {}
}

pub(crate) use imp::tls_get_addr;
pub(crate) use imp::ElfTls;
