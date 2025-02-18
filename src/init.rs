use crate::{
    dl_iterate_phdr::CallBack,
    register::{global_find, register, DylibState, MANAGER},
    DlPhdrInfo, OpenFlags, Result,
};
use core::{
    ffi::{c_char, c_int, c_void, CStr},
    ptr::{addr_of, addr_of_mut, null_mut, NonNull},
};
use elf_loader::{
    abi::{PT_DYNAMIC, PT_LOAD},
    arch::{Dyn, Phdr},
    dynamic::ElfRawDynamic,
    segment::{ElfSegments, MASK, PAGE_SIZE},
    set_global_scope, RelocatedDylib, Symbol, UserData,
};
use spin::Once;
use std::{env, ffi::CString, os::unix::ffi::OsStringExt, path::PathBuf, sync::Arc};

#[repr(C)]
pub(crate) struct LinkMap {
    pub l_addr: *mut c_void,
    pub l_name: *const c_char,
    pub l_ld: *mut Dyn,
    pub l_next: *mut LinkMap,
    pub l_prev: *mut LinkMap,
}

#[repr(C)]
pub(crate) struct GDBDebug {
    pub version: c_int,
    pub map: *mut LinkMap,
    pub brk: extern "C" fn(),
    pub state: c_int,
    pub ldbase: *mut c_void,
}

#[cfg(target_env = "gnu")]
#[inline]
fn get_debug_struct() -> &'static mut GDBDebug {
    extern "C" {
        static mut _r_debug: GDBDebug;
    }
    unsafe { &mut *addr_of_mut!(_r_debug) }
}

// 静态链接的musl中没有_dl_debug_addr这个符号，无法通过编译，因此需要生成dyn格式的可执行文件
#[cfg(target_env = "musl")]
#[inline]
fn get_debug_struct() -> &'static mut GDBDebug {
    extern "C" {
        static mut _dl_debug_addr: GDBDebug;
    }
    unsafe { &mut *addr_of_mut!(_dl_debug_addr) }
}

static ONCE: Once = Once::new();
static mut PROGRAM_NAME: Option<PathBuf> = None;

pub(crate) static mut ARGC: usize = 0;
pub(crate) static mut ARGV: Vec<*mut i8> = Vec::new();
pub(crate) static mut ENVP: usize = 0;

extern "C" {
    static environ: usize;
}

pub(crate) unsafe fn from_raw(
    name: CString,
    base: usize,
    dynamic_ptr: *const Dyn,
    phdrs: Option<&'static [Phdr]>,
) -> Result<Option<RelocatedDylib<'static>>> {
    let dynamic = ElfRawDynamic::new(dynamic_ptr)?;
    let offset = if dynamic.hash_off > base { 0 } else { base };
    #[allow(unused_mut)]
    let mut dynamic = dynamic.finish(offset);
    #[cfg(feature = "version")]
    {
        dynamic.verneed = dynamic
            .verneed
            .map(|(off, num)| (off.checked_add(base - offset).unwrap_unchecked(), num));
        dynamic.verdef = dynamic
            .verdef
            .map(|(off, num)| (off.checked_add(base - offset).unwrap_unchecked(), num));
    }
    #[allow(unused_mut)]
    let mut user_data = UserData::empty();
    unsafe fn drop_handle(_handle: NonNull<c_void>, _len: usize) -> elf_loader::Result<()> {
        Ok(())
    }
    let memory = if let Some(memory) = NonNull::new(base as _) {
        memory
    } else {
        // 如果程序本身不是Shared object file,那么它的这个字段为0,此时无法使用程序本身的符号进行重定位
        log::warn!(
            "Failed to initialize an existing library: [{:?}], Because it's not a Shared object file",
            (*addr_of!(PROGRAM_NAME)).as_ref().unwrap()
        );
        return Ok(None);
    };
    let len = if let Some(phdrs) = phdrs {
        let mut min_vaddr = usize::MAX;
        let mut max_vaddr = 0;
        phdrs.iter().for_each(|phdr| {
            if phdr.p_type == PT_LOAD {
                min_vaddr = min_vaddr.min(phdr.p_vaddr as usize & MASK);
                max_vaddr = max_vaddr
                    .max((phdr.p_vaddr as usize + phdr.p_memsz as usize + PAGE_SIZE - 1) & MASK);
            }
        });
        max_vaddr - min_vaddr
    } else {
        0
    };
    let segments = ElfSegments::new(memory, len, drop_handle);
    #[cfg(feature = "debug")]
    unsafe {
        if phdrs.is_some() {
            use super::debug::*;
            user_data.insert(
                crate::loader::DEBUG_INFO_ID,
                Box::new(DebugInfo::new(offset, name.as_ptr(), dynamic_ptr as usize)),
            );
        }
    };
    let lib = RelocatedDylib::new_uncheck(
        name,
        base,
        dynamic,
        phdrs.unwrap_or(&[]),
        segments,
        user_data,
    );
    Ok(Some(lib))
}

type IterPhdr = extern "C" fn(callback: Option<CallBack>, data: *mut c_void) -> c_int;

// 寻找libc中的dl_iterate_phdr函数
fn iterate_phdr(start: *const LinkMap, mut f: impl FnMut(Symbol<IterPhdr>)) {
    let mut cur_map_ptr = start;
    while !cur_map_ptr.is_null() {
        let cur_map = unsafe { &*cur_map_ptr };
        let name = unsafe { CStr::from_ptr(cur_map.l_name).to_owned() };
        if let Some(lib) =
            unsafe { from_raw(name, cur_map.l_addr as usize, cur_map.l_ld, None).unwrap() }
        {
            if lib.name().contains("libc.so") {
                f(unsafe { lib.get::<IterPhdr>("dl_iterate_phdr").unwrap() });
                return;
            }
        };
        cur_map_ptr = cur_map.l_next;
    }
    panic!("can not find libc's dl_iterate_phdr");
}

fn init_argv() {
    let mut argv = Vec::new();
    for arg in env::args_os() {
        argv.push(CString::new(arg.into_vec()).unwrap().into_raw());
    }
    argv.push(null_mut());
    unsafe {
        ARGC = argv.len();
        ARGV = argv;
        ENVP = environ;
    }
}

unsafe extern "C" fn callback(info: *mut DlPhdrInfo, _size: usize, _data: *mut c_void) -> c_int {
    let info = unsafe { &*info };
    let base = info.dlpi_addr as usize;
    let phdrs = core::slice::from_raw_parts(info.dlpi_phdr, info.dlpi_phnum as usize);
    let dynamic_ptr = phdrs
        .iter()
        .find_map(|phdr| {
            if phdr.p_type == PT_DYNAMIC {
                Some(base + phdr.p_vaddr as usize)
            } else {
                None
            }
        })
        .unwrap() as _;
    let Some(lib) = from_raw(
        CStr::from_ptr(info.dlpi_name).to_owned(),
        base,
        dynamic_ptr,
        Some(core::mem::transmute(phdrs)),
    )
    .unwrap() else {
        return 0;
    };
    let flags = OpenFlags::RTLD_NODELETE | OpenFlags::RTLD_GLOBAL;
    let deps = Some(Arc::new(vec![lib.clone()].into_boxed_slice()));
    let start = lib.base();
    let end = start + lib.map_len();
    let shortname = lib.shortname();
    let name = if shortname == "" {
        (*addr_of!(PROGRAM_NAME))
            .as_ref()
            .unwrap()
            .to_str()
            .unwrap()
    } else {
        shortname
    };
    log::info!(
        "Initialize an existing library: [{:?}] [{:#x}]-[{:#x}]",
        name,
        start,
        end,
    );

    register(
        lib,
        flags,
        deps,
        &mut MANAGER.write(),
        *DylibState::default().set_relocated(),
    );
    0
}
/// `init` is responsible for the initialization of dlopen_rs, If you want to use the dynamic library that the program itself depends on,
/// or want to use the debug function, please call it at the beginning. This is usually necessary.
pub fn init() {
    ONCE.call_once(|| {
        init_argv();
        let program_self = env::current_exe().unwrap();
        unsafe { PROGRAM_NAME = Some(program_self) };
        let debug = get_debug_struct();
        iterate_phdr(debug.map, |iter| {
            #[cfg(feature = "debug")]
            crate::debug::init_debug(debug);
            iter(Some(callback), null_mut());
        });
        unsafe { set_global_scope(global_find as _) };
        log::info!("Initialization is complete");
    });
}
