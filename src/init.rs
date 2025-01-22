use crate::{
    register::{global_find, register, MANAGER},
    Dylib, OpenFlags, Result,
};
use core::{
    ffi::{c_char, c_int, c_void, CStr},
    marker::PhantomData,
    ptr::{addr_of, addr_of_mut, null_mut, NonNull},
};
use elf_loader::{
    arch::Dyn, dynamic::ElfRawDynamic, segment::ElfSegments, set_global_scope, RelocatedDylib,
    UserData,
};
use spin::Once;
use std::{env, path::PathBuf, sync::Arc};

#[repr(C)]
pub(crate) struct LinkMap {
    pub l_addr: *mut c_void,
    pub l_name: *const c_char,
    pub l_ld: *mut Dyn,
    pub l_next: *mut LinkMap,
    pub l_prev: *mut LinkMap,
}

#[repr(C)]
pub(crate) struct Debug {
    pub version: c_int,
    pub map: *mut LinkMap,
    pub brk: extern "C" fn(),
    pub state: c_int,
    pub ldbase: *mut c_void,
}

extern "C" {
    static mut _r_debug: Debug;
}

pub(crate) static mut OLD_DL_ITERATE_PHDR: Option<
    extern "C" fn(
        callback: Option<
            unsafe extern "C" fn(
                info: *mut libc::dl_phdr_info,
                size: libc::size_t,
                data: *mut c_void,
            ) -> c_int,
        >,
        data: *mut c_void,
    ) -> c_int,
> = None;

static ONCE: Once = Once::new();
static mut PROGRAM_NAME: Option<PathBuf> = None;

pub(crate) unsafe fn from_link_map(link_map: &LinkMap) -> Result<Option<Dylib>> {
    let dynamic = ElfRawDynamic::new(link_map.l_ld)?;
    let base = if dynamic.hash_off > link_map.l_addr as usize {
        0
    } else {
        link_map.l_addr as usize
    };
	#[allow(unused_mut)]
    let mut dynamic = dynamic.finish(base);
    #[cfg(feature = "version")]
    {
        dynamic.verneed = dynamic.verneed.map(|(off, num)| {
            (
                off.checked_add(link_map.l_addr as usize - base)
                    .unwrap_unchecked(),
                num,
            )
        });
        dynamic.verdef = dynamic.verdef.map(|(off, num)| {
            (
                off.checked_add(link_map.l_addr as usize - base)
                    .unwrap_unchecked(),
                num,
            )
        });
    }
    #[allow(unused_mut)]
    let mut user_data = UserData::empty();
    let name = CStr::from_ptr(link_map.l_name).to_str().unwrap();
    if name == "" {
        log::info!(
            "Initialize an existing library: [{:?}]",
            (*addr_of!(PROGRAM_NAME)).as_ref().unwrap()
        );
    } else {
        log::info!(
            "Initialize an existing library: [{}]",
            CStr::from_ptr(link_map.l_name).to_str().unwrap()
        );
    }
    unsafe fn drop_handle(_handle: NonNull<c_void>, _len: usize) -> elf_loader::Result<()> {
        Ok(())
    }
    let memory = if let Some(memory) = NonNull::new(link_map.l_addr) {
        memory
    } else {
        // 如果程序本身不是Shared object file,那么它的这个字段为0,此时无法使用程序本身的符号进行重定位
        log::warn!(
            "Failed to initialize an existing library: [{:?}], Because it's not a Shared object file",
            (*addr_of!(PROGRAM_NAME)).as_ref().unwrap()
        );
        return Ok(None);
    };
    let segments = ElfSegments::new(memory, 0, drop_handle);
    #[cfg(feature = "debug")]
    unsafe {
        use super::debug::*;
        user_data.insert(
            crate::loader::DEBUG_INFO_ID,
            Box::new(DebugInfo::new(
                link_map.l_addr as usize,
                link_map.l_name as _,
                link_map.l_ld as usize,
            )),
        );
    };
    let lib = RelocatedDylib::new_uncheck(
        CStr::from_ptr(link_map.l_name).to_owned(),
        link_map.l_addr as usize,
        dynamic,
        &[],
        segments,
        user_data,
    );
    let flags = OpenFlags::RTLD_NODELETE | OpenFlags::RTLD_GLOBAL;
    let dylib = Dylib {
        inner: lib.core_component().clone(),
        flags,
        deps: Some(Arc::new(Box::new([lib.core_component().clone()]))),
        _marker: PhantomData,
    };

    register(dylib.clone(), &mut MANAGER.write(), false, None);
    Ok(Some(dylib))
}

pub fn init() {
    ONCE.call_once(|| {
        let program_self = env::current_exe().unwrap();
        unsafe { PROGRAM_NAME = Some(program_self) };
        let debug = unsafe { &mut *addr_of_mut!(_r_debug) };
        let mut cur_map_ptr = debug.map;
        debug.map = null_mut();
        #[cfg(feature = "debug")]
        {
            let mut custom = crate::debug::DEBUG.lock().unwrap();
            custom.debug = debug;
            custom.tail = null_mut();
            drop(custom);
        }
        while !cur_map_ptr.is_null() {
            let cur_map = unsafe { &*cur_map_ptr };
            unsafe { from_link_map(cur_map).unwrap() }.map(|lib| {
                if lib.name().contains("libc.so") {
                    unsafe {
                        OLD_DL_ITERATE_PHDR = Some(core::mem::transmute(
                            lib.get::<extern "C" fn(
                                callback: Option<
                                    unsafe extern "C" fn(
                                        info: *mut libc::dl_phdr_info,
                                        size: libc::size_t,
                                        data: *mut c_void,
                                    )
                                        -> c_int,
                                >,
                                data: *mut c_void,
                            ) -> c_int>("dl_iterate_phdr")
                                .unwrap()
                                .into_raw(),
                        ))
                    };
                }
            });

            cur_map_ptr = cur_map.l_next;
        }

        unsafe { set_global_scope(global_find as _) };
        log::info!("Initialization is complete");
    });
}
