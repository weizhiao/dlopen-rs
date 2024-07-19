use std::ffi::c_int;

#[cfg(feature = "tls")]
use crate::tls::tls_get_addr;

#[cfg(not(feature = "tls"))]
fn tls_get_addr() {}

extern "C" {
    fn _ITM_registerTMCloneTable();
    fn _ITM_deregisterTMCloneTable();
    fn __gmon_start__();
}

extern "C" fn __cxa_thread_atexit_impl() -> c_int {
    0
}

pub(crate) const BUILTIN: phf::Map<&'static str, *const ()> = phf::phf_map!(
    "__cxa_finalize"=>0 as _, //该符号需要覆盖，否则会段错误，这是因为我们不走libc的动态库管理
    "__cxa_thread_atexit_impl" =>__cxa_thread_atexit_impl as _,
    "__tls_get_addr"=> tls_get_addr as _,
    "_ITM_registerTMCloneTable"=> 0 as _,
    "_ITM_deregisterTMCloneTable"=> 0 as _,
    "__gmon_start__"=> 0 as _,
);
