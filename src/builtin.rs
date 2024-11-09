use core::ffi::c_int;

#[cfg(feature = "tls")]
use super::loader::tls_get_addr;
#[cfg(feature = "std")]
use crate::register::dl_iterate_phdr_impl;

#[cfg(not(feature = "std"))]
fn dl_iterate_phdr_impl() {}

#[cfg(not(feature = "tls"))]
fn tls_get_addr() {}

extern "C" fn __cxa_thread_atexit_impl() -> c_int {
    0
}

pub(crate) const BUILTIN: phf::Map<&'static str, *const ()> = phf::phf_map!(
    "__cxa_finalize"=>0 as _,
    "__cxa_thread_atexit_impl" =>__cxa_thread_atexit_impl as _,
    "__tls_get_addr"=> tls_get_addr as _,
    "_ITM_registerTMCloneTable"=> 0 as _,
    "_ITM_deregisterTMCloneTable"=> 0 as _,
    "__gmon_start__"=> 0 as _,
    "dl_iterate_phdr"=> dl_iterate_phdr_impl as _,
);
