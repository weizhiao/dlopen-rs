//! c interface

use crate::{loader::find_symbol, register::MANAGER, ElfLibrary, OpenFlags};
use core::{
    ffi::{c_char, c_int, c_void, CStr},
    mem::forget,
    ptr::null,
};
use elf_loader::RelocatedDylib;
use libc::dl_phdr_info;
use std::sync::Arc;

/// It is the same as `dl_iterate_phdr`.
pub unsafe extern "C" fn dl_iterate_phdr(
    callback: Option<
        unsafe extern "C" fn(
            info: *mut libc::dl_phdr_info,
            size: libc::size_t,
            data: *mut libc::c_void,
        ) -> libc::c_int,
    >,
    data: *mut libc::c_void,
) -> libc::c_int {
    let mut ret = 0;
    let reader = MANAGER.read();
    for lib in reader.all.values() {
        let phdrs = lib.relocated_dylib_ref().phdrs();
        if phdrs.is_empty() {
            continue;
        }
        let mut info = dl_phdr_info {
            dlpi_addr: lib.relocated_dylib_ref().base() as _,
            dlpi_name: lib.relocated_dylib_ref().cname().as_ptr(),
            dlpi_phdr: phdrs.as_ptr().cast(),
            dlpi_phnum: phdrs.len() as _,
            dlpi_adds: reader.all.len() as _,
            dlpi_subs: 0,
            dlpi_tls_modid: 0,
            dlpi_tls_data: core::ptr::null_mut(),
        };
        if let Some(callback) = callback {
            ret = callback(&mut info, size_of::<dl_phdr_info>(), data);
            if ret != 0 {
                break;
            }
        }
    }
    ret
}

/// It is the same as `dlopen`.
pub unsafe extern "C" fn dlopen(filename: *const c_char, flags: c_int) -> *const c_void {
    let mut lib = if filename.is_null() {
        MANAGER.read().all.get_index(0).unwrap().1.get_dylib()
    } else {
        let flags = OpenFlags::from_bits_retain(flags as _);
        let filename = core::ffi::CStr::from_ptr(filename);
        let path = filename.to_str().unwrap();
        if let Ok(lib) = ElfLibrary::dlopen(path, flags) {
            lib
        } else {
            return null();
        }
    };
    Arc::into_raw(core::mem::take(&mut lib.deps).unwrap()) as _
}

/// It is the same as `dlsym`.
pub unsafe extern "C" fn dlsym(handle: *const c_void, symbol_name: *const c_char) -> *const c_void {
    const RTLD_DEFAULT: usize = 0;
    const RTLD_NEXT: usize = usize::MAX;
    let value = handle as usize;
    let name = CStr::from_ptr(symbol_name).to_str().unwrap_unchecked();
    let sym = if value == RTLD_DEFAULT {
        log::info!("dlsym: Use RTLD_DEFAULT flag to find symbol [{}]", name);
        MANAGER
            .read()
            .global
            .values()
            .find_map(|lib| lib.get::<()>(name).map(|v| v.into_raw()))
    } else if value == RTLD_NEXT {
        todo!("RTLD_NEXT is not supported")
    } else {
        let libs = Arc::from_raw(handle as *const Box<[RelocatedDylib<'static>]>);
        let symbol = find_symbol::<()>(&libs, name)
            .ok()
            .map(|sym| sym.into_raw());
        forget(libs);
        symbol
    };
    sym.unwrap_or(null()).cast()
}

/// It is the same as `dlclose`.
pub unsafe extern "C" fn dlclose(handle: *const c_void) -> c_int {
    let deps = Arc::from_raw(handle as *const Box<[RelocatedDylib<'static>]>);
    let dylib = MANAGER
        .read()
        .all
        .get(deps[0].shortname())
        .unwrap()
        .get_dylib();
    drop(deps);
    log::info!("dlclose: Closing [{}]", dylib.name());
    0
}

/// It is the same as `dladdr`.
pub unsafe extern "C" fn dladdr(addr: *const c_void, info: *mut libc::Dl_info) -> c_int {
    println!("{:?}", addr);
    if let Some(dl_info) = ElfLibrary::dladdr(addr as usize) {
        let info = &mut *info;
        info.dli_fbase = dl_info.dylib().base() as _;
        info.dli_fname = dl_info.dylib().cname().as_ptr();
        info.dli_saddr = dl_info.symbol_addr().unwrap_or(0) as _;
        info.dli_sname = dl_info.symbol_name().map_or(null(), |s| s.as_ptr());
        1
    } else {
        0
    }
}
