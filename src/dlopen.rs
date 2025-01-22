use crate::{
    find_lib_error,
    loader::{builtin, create_lazy_scope, deal_unknown, find_symbol, Dylib, ElfLibrary},
    register::{register, IS_RELOCATED, MANAGER},
    OpenFlags, Result,
};
use alloc::{format, vec::Vec};
use core::{
    ffi::{c_char, c_int, c_void, CStr},
    marker::PhantomData,
    mem::forget,
    ptr::null,
};
use elf_loader::CoreComponent;
use std::sync::Arc;

static LD_LIBRARY_PATH: std::sync::OnceLock<Vec<std::path::PathBuf>> = std::sync::OnceLock::new();
//static LAZY_BIND: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();

const RTLD_DEFAULT: usize = 0;
const RTLD_NEXT: usize = usize::MAX;

impl ElfLibrary {
    /// Load a shared library from a specified path
    ///
    /// # Note
    /// * Please set the `LD_LIBRARY_PATH` environment variable before calling this function.
    /// dlopen-rs will look for dependent dynamic libraries in the paths saved in `LD_LIBRARY_PATH`. In addition, dlopen-rs also looks for dependent dynamic libraries in the registered dynamic library  
    /// * Lazy binding is forcibly turned on when `LD_BIND_NOW = 0`, forcibly turned off when `LD_BIND_NOW = 1`,
    /// and determined by the compilation parameter of the dynamic library itself when `LD_BIND_NOW` environment variable is not set.
    ///
    /// # Example
    /// ```no_run
    /// use std::path::Path;
    /// use dlopen_rs::ELFLibrary;
    ///
    /// let path = Path::new("/path/to/library.so");
    /// let lib = ELFLibrary::dlopen(path).expect("Failed to load library");
    /// ```
    pub fn dlopen(path: impl AsRef<std::ffi::OsStr>, flags: OpenFlags) -> Result<Dylib<'static>> {
        // let lazy_bind = LAZY_BIND.get_or_init(|| {
        //     std::env::var("LD_BIND_NOW")
        //         .ok()
        //         .map(|val| {
        //             val.parse::<u8>()
        //                 .expect("set LD_BIND_NOW = 0 or set LD_BIND_NOW = 1")
        //         })
        //         .map(|val| val == 0)
        // });
        let shortname = path.as_ref().to_str().unwrap().split('/').last().unwrap();
        log::info!(
            "dlopen: Try to open [{}] with [{:?}] ",
            path.as_ref().to_str().unwrap(),
            flags
        );
        let reader = MANAGER.read();
        // 检查是否是已经加载的库
        let core = if let Some(lib) = reader.all.get(shortname) {
            if lib.dylib.deps.is_some()
                && !flags
                    .difference(lib.dylib.flags)
                    .contains(OpenFlags::RTLD_GLOBAL)
            {
                return Ok(lib.dylib.clone());
            }
            lib.dylib.inner.clone()
        } else {
            let lib = ElfLibrary::from_file(path.as_ref(), flags)?;
            drop(reader);
            return dlopen_impl(
                unsafe { lib.dylib.core_component().clone() },
                flags,
                Some(lib),
            );
        };
        drop(reader);
        dlopen_impl(core, flags, None)
    }

    // /// Load a shared library from bytes
    // /// # Note
    // /// Please set the `LD_LIBRARY_PATH` environment variable before calling this function.
    // /// dlopen-rs will look for dependent dynamic libraries in the paths saved in `LD_LIBRARY_PATH`.
    // /// In addition, dlopen-rs also looks for dependent dynamic libraries in the registered dynamic library
    // // pub fn dlopen_from_binary(bytes: &[u8], name: impl AsRef<str>) -> Result<Dylib> {
    // //     #[cfg(feature = "std")]
    // //     let lazy_bind = LAZY_BIND.get_or_init(|| {
    // //         std::env::var("LD_BIND_NOW")
    // //             .ok()
    // //             .map(|val| {
    // //                 val.parse::<u8>()
    // //                     .expect("set LD_BIND_NOW = 0 or set LD_BIND_NOW = 1")
    // //             })
    // //             .map(|val| val == 0)
    // //     });
    // //     #[cfg(not(feature = "std"))]
    // //     let lazy_bind = None;
    // //     // 检查是否是已经加载的库
    // //     if let Some(lib) = MANAGER
    // //         .read()
    // //         .all()
    // //         .get(name.as_ref())
    // //         .map(|lib| lib.inner.clone())
    // //     {
    // //         return Ok(Dylib { inner: lib });
    // //     };
    // //     let lock = LOCK.lock();
    // //     let lib = ElfLibrary::from_binary(bytes, name, lazy_bind.clone())?
    // //         .register(true)
    // //         .dlopen_impl()?;
    // //     drop(lock);
    // //     Ok(lib)
    // // }
}

fn dlopen_impl(
    core: CoreComponent,
    flags: OpenFlags,
    raw: Option<ElfLibrary>,
) -> Result<Dylib<'static>> {
    LD_LIBRARY_PATH.get_or_init(|| {
        const SYS_PATH: &str = ":/lib:/usr/local/lib:/usr/lib:/lib/x86_64-linux-gnu";
        let mut ld_library_path = std::env::var("LD_LIBRARY_PATH").unwrap_or(String::new());
        ld_library_path.push_str(SYS_PATH);

        ld_library_path
            .split(":")
            .map(|str| std::path::PathBuf::try_from(str).unwrap())
            .collect()
    });
    if flags.contains(OpenFlags::CUSTOM_NOT_REGISTER) {
        log::warn!("Dlopen ignores the open flag CUSTOM_NOT_REGISTER");
    }
    let mut cur_pos = 0;
    // 用于保存所有的依赖库
    let mut dep_libs = Vec::new();
    // 新加载的动态库
    let mut new_libs = Vec::new();
    if let Some(raw) = raw {
        new_libs.push(Some(raw));
    }
    dep_libs.push(core);
    let mut lock = MANAGER.write();
    // 广度优先搜索，这是规范的要求，这个循环里会加载所有需要的动态库，无论是直接依赖还是间接依赖的
    while cur_pos < dep_libs.len() {
        let lib_names: &[&str] = unsafe { core::mem::transmute(dep_libs[cur_pos].needed_libs()) };
        for lib_name in lib_names {
            if let Some(lib) = lock.all.get_mut(*lib_name) {
                if !lib.is_mark {
                    lib.is_mark = true;
                    dep_libs.push(lib.dylib.inner.clone());
                    if flags
                        .difference(lib.dylib.flags)
                        .contains(OpenFlags::RTLD_GLOBAL)
                    {
                        let shortname = lib.dylib.inner.shortname().to_owned();
                        log::debug!(
							"Trying to update a library. Name: [{}] Old flags:[{:?}] New flags:[{:?}]",
							shortname,
							lib.dylib.flags,
							flags
						);
                        lib.dylib.flags = flags;
                        let core = lib.dylib.inner.clone();
                        lock.global.insert(shortname, core);
                    }
                }
                continue;
            }

            let mut is_find = false;
            for sys_path in LD_LIBRARY_PATH.get().unwrap() {
                let file_path = sys_path.join(lib_name);
                match std::fs::File::open(&file_path) {
                    Ok(file) => {
                        let new_lib =
                            ElfLibrary::from_open_file(file, file_path.to_str().unwrap(), flags)?;
                        let inner = unsafe { new_lib.dylib.core_component().clone() };
                        let dylib = Dylib {
                            inner: inner.clone(),
                            flags,
                            deps: None,
                            _marker: PhantomData,
                        };
                        // 最多一次性加载255个新库
                        assert!(new_libs.len() < IS_RELOCATED as usize);
                        register(dylib, &mut lock, true, Some(new_libs.len() as _));
                        dep_libs.push(inner);
                        new_libs.push(Some(new_lib));
                        is_find = true;
                        break;
                    }
                    Err(_) => continue,
                }
            }
            if !is_find {
                return Err(find_lib_error(format!("can not find file: {}", lib_name)));
            }
        }
        cur_pos += 1;
    }

    #[derive(Clone, Copy)]
    struct Item {
        idx: usize,
        next: usize,
    }
    // 保存new_libs的索引
    let mut stack = Vec::new();
    stack.push(Item { idx: 0, next: 0 });

    let deps = Arc::new(dep_libs.into_boxed_slice());
    while let Some(mut item) = stack.pop() {
        let names = new_libs[item.idx].as_ref().unwrap().needed_libs();
        let mut can_relocate = true;
        for name in names.iter().skip(item.next) {
            let lib = lock.all.get_mut(*name).unwrap();
            let idx = lib.new_idx;
            lib.is_mark = false;
            item.next += 1;
            if idx == IS_RELOCATED {
                continue;
            }
            lib.new_idx = IS_RELOCATED;
            stack.push(item);
            stack.push(Item {
                idx: idx as usize,
                next: 0,
            });
            can_relocate = false;
            break;
        }
        if can_relocate {
            let iter = lock.global.values().chain(deps.iter());

            let reloc = |lib: ElfLibrary| {
                log::debug!("Relocating dylib [{}]", lib.name());
                let lazy_scope = create_lazy_scope(&deps, lib.dylib.is_lazy());
                lib.dylib
                    .relocate(
                        iter,
                        &|name| builtin::BUILTIN.get(name).copied(),
                        deal_unknown,
                        lazy_scope,
                    )
                    .map(|lib| lib.into_core_component())
            };
            reloc(core::mem::take(&mut new_libs[item.idx]).unwrap())?;
        }
    }

    let res = Dylib {
        inner: deps[0].clone(),
        flags,
        deps: Some(deps),
        _marker: PhantomData,
    };
    //重新注册因为更新了deps
    register(res.clone(), &mut lock, false, None);
    Ok(res)
}

pub unsafe fn dlopen(filename: *const c_char, flags: c_int) -> *const c_void {
    let mut lib = if filename.is_null() {
        MANAGER.read().all.get_index(0).unwrap().1.dylib.clone()
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

pub unsafe fn dlsym(handle: *const c_void, symbol_name: *const c_char) -> *const c_void {
    let value = handle as usize;
    let name = CStr::from_ptr(symbol_name).to_str().unwrap_unchecked();
    let sym = if value == RTLD_DEFAULT {
        log::info!("dlsym: Use RTLD_DEFAULT flag to find symbol [{}]", name);
        MANAGER
            .read()
            .global
            .values()
            .find_map(|lib| lib.get::<()>(name).ok().map(|v| v.into_raw()))
    } else if value == RTLD_NEXT {
        todo!("RTLD_NEXT is not supported")
    } else {
        let libs = Arc::from_raw(handle as *const Box<[CoreComponent]>);
        let symbol = find_symbol::<()>(&libs, name)
            .ok()
            .map(|sym| sym.into_raw());
        forget(libs);
        symbol
    };
    sym.unwrap_or(null()).cast()
}

pub unsafe fn dlclose(handle: *const c_void) -> c_int {
    let libs = Arc::from_raw(handle as *const Box<[Dylib]>);
    log::info!("dlclose: Closing [{}]", libs[0].name());
    0
}
