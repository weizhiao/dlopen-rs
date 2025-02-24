//! c interface

use crate::register::MANAGER;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::ffi::{c_int, c_void};
use elf_loader::RelocatedDylib;

pub use crate::dl_iterate_phdr::{dl_iterate_phdr, CDlPhdrInfo};
pub use crate::dladdr::{dladdr, CDlinfo};
pub use crate::dlopen::dlopen;
pub use crate::dlsym::dlsym;

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
