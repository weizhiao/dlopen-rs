use dlopen_rs::dlopen::dl_iterate_phdr as dl_iterate_phdr_impl;
use std::{
    ffi::{c_char, c_int, c_void},
    ptr::null,
};

#[ctor::ctor]
fn init() {
    env_logger::init();
    dlopen_rs::init();
}

#[no_mangle]
unsafe extern "C" fn dlopen(filename: *const c_char, flags: c_int) -> *const c_void {
    dlopen_rs::dlopen::dlopen(filename, flags)
}

#[no_mangle]
unsafe extern "C" fn dlsym(handle: *const c_void, symbol_name: *const c_char) -> *const c_void {
    dlopen_rs::dlopen::dlsym(handle, symbol_name)
}

#[no_mangle]
unsafe extern "C" fn dlclose(handle: *const c_void) -> c_int {
    dlopen_rs::dlopen::dlclose(handle)
}

#[no_mangle]
unsafe extern "C" fn dl_iterate_phdr(
    callback: Option<
        unsafe extern "C" fn(
            info: *mut libc::dl_phdr_info,
            size: libc::size_t,
            data: *mut libc::c_void,
        ) -> libc::c_int,
    >,
    data: *mut libc::c_void,
) -> c_int {
    dl_iterate_phdr_impl(callback, data)
}

#[no_mangle]
unsafe extern "C" fn dladdr(_addr: *const c_void, _info: *mut libc::Dl_info) {
    todo!()
}

#[no_mangle]
unsafe extern "C" fn dlinfo(_handle: *const c_void, _request: c_int, _info: *mut c_void) {
    todo!()
}

#[no_mangle]
unsafe extern "C" fn dlerror() -> *const c_char {
    null()
}
