use crate::{register::MANAGER, ElfLibrary, Error, Result};
use alloc::boxed::Box;
use core::ffi::{c_char, c_int, c_ulonglong, c_void};
use elf_loader::arch::Phdr;

/// same as dl_phdr_info in libc
#[repr(C)]
pub struct DlPhdrInfo {
    pub dlpi_addr: usize,
    pub dlpi_name: *const c_char,
    pub dlpi_phdr: *const Phdr,
    pub dlpi_phnum: u16,
    pub dlpi_adds: c_ulonglong,
    pub dlpi_subs: c_ulonglong,
    pub dlpi_tls_modid: usize,
    pub dlpi_tls_data: *mut c_void,
}

impl ElfLibrary {
    /// Iterate over the program headers of all dynamic libraries.
    pub fn dl_iterate_phdr<F>(mut callback: F) -> Result<()>
    where
        F: FnMut(&DlPhdrInfo) -> Result<()>,
    {
        let reader = MANAGER.read();
        for lib in reader.all.values() {
            let phdrs = lib.relocated_dylib_ref().phdrs();
            if phdrs.is_empty() {
                continue;
            }
            let info = DlPhdrInfo {
                dlpi_addr: lib.relocated_dylib_ref().base() as _,
                dlpi_name: lib.relocated_dylib_ref().cname().as_ptr(),
                dlpi_phdr: phdrs.as_ptr().cast(),
                dlpi_phnum: phdrs.len() as _,
                dlpi_adds: reader.all.len() as _,
                dlpi_subs: 0,
                dlpi_tls_modid: 0,
                dlpi_tls_data: core::ptr::null_mut(),
            };
            callback(&info)?;
        }
        Ok(())
    }
}

pub(crate) type CallBack =
    unsafe extern "C" fn(info: *mut DlPhdrInfo, size: usize, data: *mut c_void) -> c_int;

// It is the same as `dl_iterate_phdr`.
pub extern "C" fn dl_iterate_phdr(callback: Option<CallBack>, data: *mut c_void) -> c_int {
    let f = |info: &DlPhdrInfo| {
        if let Some(callback) = callback {
            unsafe {
                let ret = callback(
                    info as *const DlPhdrInfo as _,
                    size_of::<DlPhdrInfo>(),
                    data,
                );
                if ret != 0 {
                    return Err(Error::IteratorPhdrError { err: Box::new(ret) });
                }
            };
        }
        Ok(())
    };
    if let Err(Error::IteratorPhdrError { err }) = ElfLibrary::dl_iterate_phdr(f) {
        *err.downcast::<i32>().unwrap()
    } else {
        0
    }
}
