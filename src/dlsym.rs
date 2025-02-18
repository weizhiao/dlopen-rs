use crate::{loader::find_symbol, register::MANAGER};
use alloc::{boxed::Box, sync::Arc};
use core::{
    ffi::{c_char, c_void, CStr},
    mem::forget,
    ptr::null,
};
use elf_loader::RelocatedDylib;

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
