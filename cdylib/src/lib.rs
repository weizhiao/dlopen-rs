use dlopen_rs::dl_iterate_phdr_impl;
use std::ffi::{c_char, c_int, c_void};

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
