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

#[cfg(not(feature = "unwinding"))]
pub(crate) const BUILTIN: phf::Map<&'static str, *const ()> = phf::phf_map!(
    "__cxa_finalize"=>0 as _,
    "__cxa_thread_atexit_impl" =>__cxa_thread_atexit_impl as _,
    "__tls_get_addr"=> tls_get_addr as _,
    "_ITM_registerTMCloneTable"=> 0 as _,
    "_ITM_deregisterTMCloneTable"=> 0 as _,
    "__gmon_start__"=> 0 as _,
    "dl_iterate_phdr"=> dl_iterate_phdr_impl as _,
);

#[cfg(feature = "unwinding")]
pub(crate) const BUILTIN: phf::Map<&'static str, *const ()> = phf::phf_map!(
    "__cxa_thread_atexit_impl" =>__cxa_thread_atexit_impl as _,
    "__tls_get_addr"=> tls_get_addr as _,
    "_ITM_registerTMCloneTable"=> 0 as _,
    "_ITM_deregisterTMCloneTable"=> 0 as _,
    "__gmon_start__"=> 0 as _,
    "dl_iterate_phdr"=> dl_iterate_phdr_impl as _,
    "_Unwind_Backtrace" => unwinding::abi::_Unwind_Backtrace as _,
    "_Unwind_ForcedUnwind" => unwinding::abi::_Unwind_ForcedUnwind as _,
    "_Unwind_GetLanguageSpecificData" => unwinding::abi::_Unwind_GetLanguageSpecificData as _,
    "_Unwind_GetDataRelBase" => unwinding::abi::_Unwind_GetDataRelBase as _,
    "_Unwind_FindEnclosingFunction" => unwinding::abi::_Unwind_FindEnclosingFunction as _,
    "_Unwind_GetGR" => unwinding::abi::_Unwind_GetGR as _,
    "_Unwind_GetIP" => unwinding::abi::_Unwind_GetIP as _,
    "_Unwind_GetIPInfo" => unwinding::abi::_Unwind_GetIPInfo as _,
    "_Unwind_Resume" => unwinding::abi::_Unwind_Resume as _,
    "_Unwind_SetGR" => unwinding::abi::_Unwind_SetGR as _,
    "_Unwind_SetIP" => unwinding::abi::_Unwind_SetIP as _,
    "_Unwind_DeleteException" => unwinding::abi::_Unwind_DeleteException as _,
    "_Unwind_GetCFA" => unwinding::abi::_Unwind_GetCFA as _,
    "_Unwind_GetRegionStart" => unwinding::abi::_Unwind_GetRegionStart as _,
    "_Unwind_GetTextRelBase" => unwinding::abi::_Unwind_GetTextRelBase as _,
    "_Unwind_RaiseException" => unwinding::abi::_Unwind_RaiseException as _,
    "_Unwind_Resume_or_Rethrow" => unwinding::abi::_Unwind_Resume_or_Rethrow as _,
);
