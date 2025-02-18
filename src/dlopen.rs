use crate::{
    loader::{builtin, create_lazy_scope, deal_unknown, Dylib, ElfLibrary},
    register::{register, DylibState, MANAGER},
    OpenFlags, Result,
};
use alloc::{borrow::ToOwned, sync::Arc, vec::Vec};
use core::ffi::{c_char, c_int, c_void};
use elf_loader::RelocatedDylib;

impl ElfLibrary {
    /// Load a shared library from a specified path. It is the same as dlopen.
    ///
    /// # Example
    /// ```no_run
    /// use std::path::Path;
    /// use dlopen_rs::ELFLibrary;
    ///
    /// let path = Path::new("/path/to/library.so");
    /// let lib = ELFLibrary::dlopen(path, OpenFlags::RTLD_LOCAL).expect("Failed to load library");
    /// ```
    #[cfg(feature = "std")]
    #[inline]
    pub fn dlopen(path: impl AsRef<std::ffi::OsStr>, flags: OpenFlags) -> Result<Dylib> {
        dlopen_impl(path.as_ref().to_str().unwrap(), flags, || {
            ElfLibrary::from_file(path.as_ref(), flags)
        })
    }

    /// Load a shared library from bytes. It is the same as dlopen. However, it can also be used in the no_std environment,
    /// and it will look for dependent libraries in those manually opened dynamic libraries.
    #[inline]
    pub fn dlopen_from_binary(
        bytes: &[u8],
        path: impl AsRef<str>,
        flags: OpenFlags,
    ) -> Result<Dylib> {
        dlopen_impl(path.as_ref(), flags, || {
            ElfLibrary::from_binary(bytes, path.as_ref(), flags)
        })
    }
}

struct Recycler {
    is_recycler: bool,
    old_all_len: usize,
    old_global_len: usize,
}

impl Drop for Recycler {
    fn drop(&mut self) {
        if self.is_recycler {
            log::debug!("Destroying newly added dynamic libraries from the global");
            let mut lock = MANAGER.write();
            lock.all.truncate(self.old_all_len);
            lock.global.truncate(self.old_global_len);
        }
    }
}

fn dlopen_impl(path: &str, flags: OpenFlags, f: impl Fn() -> Result<ElfLibrary>) -> Result<Dylib> {
    let shortname = path.split('/').last().unwrap();
    log::info!("dlopen: Try to open [{}] with [{:?}] ", path, flags);
    let reader = MANAGER.read();
    // 新加载的动态库
    let mut new_libs = Vec::new();
    let core = if flags.contains(OpenFlags::CUSTOM_NOT_REGISTER) {
        let lib = f()?;
        let core = lib.dylib.core_component().clone();
        new_libs.push(Some(lib));
        unsafe { RelocatedDylib::from_core_component(core) }
    } else {
        // 检查是否是已经加载的库
        if let Some(lib) = reader.all.get(shortname) {
            if lib.deps().is_some()
                && !flags
                    .difference(lib.flags())
                    .contains(OpenFlags::RTLD_GLOBAL)
            {
                return Ok(lib.get_dylib());
            }
            lib.relocated_dylib()
        } else {
            let lib = f()?;
            let core = lib.dylib.core_component().clone();
            new_libs.push(Some(lib));
            unsafe { RelocatedDylib::from_core_component(core) }
        }
    };

    drop(reader);

    let mut recycler = Recycler {
        is_recycler: true,
        old_all_len: usize::MAX,
        old_global_len: usize::MAX,
    };

    // 用于保存所有的依赖库
    let mut dep_libs = Vec::new();
    let mut cur_pos = 0;
    dep_libs.push(core.clone());
    let mut lock = MANAGER.write();
    recycler.old_all_len = lock.all.len();
    recycler.old_global_len = lock.global.len();

    register(core, flags, None, &mut lock, DylibState::default());

    #[cfg(feature = "std")]
    let mut cur_newlib_pos = 0;
    // 广度优先搜索，这是规范的要求，这个循环里会加载所有需要的动态库，无论是直接依赖还是间接依赖的
    while cur_pos < dep_libs.len() {
        let lib_names: &[&str] = unsafe { core::mem::transmute(dep_libs[cur_pos].needed_libs()) };
        #[cfg(feature = "std")]
        let mut cur_rpath = None;
        for lib_name in lib_names {
            if let Some(lib) = lock.all.get_mut(*lib_name) {
                if !lib.state.is_used() {
                    lib.state.set_used();
                    dep_libs.push(lib.relocated_dylib());
                    log::debug!("Use an existing dylib: [{}]", lib.shortname());
                    if flags
                        .difference(lib.flags())
                        .contains(OpenFlags::RTLD_GLOBAL)
                    {
                        let shortname = lib.shortname().to_owned();
                        log::debug!(
							"Trying to update a library. Name: [{}] Old flags:[{:?}] New flags:[{:?}]",
							shortname,
							lib.flags(),
							flags
						);
                        lib.set_flags(flags);
                        let core = lib.relocated_dylib();
                        lock.global.insert(shortname, core);
                    }
                }
                continue;
            }

            #[cfg(feature = "std")]
            {
                let rpath = if let Some(rpath) = &cur_rpath {
                    rpath
                } else {
                    let parent_lib = new_libs[cur_newlib_pos].as_ref().unwrap();
                    cur_rpath = Some(
                        parent_lib
                            .dylib
                            .rpath()
                            .map(|rpath| imp::fixup_rpath(parent_lib.name(), rpath))
                            .unwrap_or(Box::new([])),
                    );
                    cur_newlib_pos += 1;
                    unsafe { cur_rpath.as_ref().unwrap_unchecked() }
                };

                imp::find_library(rpath, lib_name, |file, file_path| {
                    let new_lib =
                        ElfLibrary::from_open_file(file, file_path.to_str().unwrap(), flags)?;
                    let inner = new_lib.dylib.core_component().clone();
                    register(
                        unsafe { RelocatedDylib::from_core_component(inner.clone()) },
                        flags,
                        None,
                        &mut lock,
                        *DylibState::default()
                            .set_used()
                            .set_new_idx(new_libs.len() as _),
                    );
                    dep_libs.push(unsafe { RelocatedDylib::from_core_component(inner) });
                    new_libs.push(Some(new_lib));
                    Ok(())
                })?;
            }

            #[cfg(not(feature = "std"))]
            return Err(crate::find_lib_error(alloc::format!(
                "can not find file: {}",
                lib_name
            )));
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

    'start: while let Some(mut item) = stack.pop() {
        let names = new_libs[item.idx].as_ref().unwrap().needed_libs();
        for name in names.iter().skip(item.next) {
            let lib = lock.all.get_mut(*name).unwrap();
            lib.state.set_unused();
            let Some(idx) = lib.state.get_new_idx() else {
                continue;
            };
            lib.state.set_relocated();
            item.next += 1;
            stack.push(item);
            stack.push(Item {
                idx: idx as usize,
                next: 0,
            });
            continue 'start;
        }
        let iter = lock.global.values().chain(dep_libs.iter());
        let reloc = |lib: ElfLibrary| {
            log::debug!("Relocating dylib [{}]", lib.name());
            let lazy_scope = create_lazy_scope(&dep_libs, lib.dylib.is_lazy());
            lib.dylib.relocate(
                iter,
                &|name| builtin::BUILTIN.get(name).copied(),
                deal_unknown,
                lazy_scope,
            )
        };
        reloc(core::mem::take(&mut new_libs[item.idx]).unwrap())?;
    }

    let deps = Arc::new(dep_libs.into_boxed_slice());
    if !flags.contains(OpenFlags::CUSTOM_NOT_REGISTER) {
        recycler.is_recycler = false;
    }
    let core = deps[0].clone();

    let res = Dylib {
        inner: core.clone(),
        flags,
        deps: Some(deps.clone()),
    };
    //重新注册因为更新了deps
    register(
        core,
        flags,
        Some(deps),
        &mut lock,
        *DylibState::default().set_relocated(),
    );
    Ok(res)
}

#[cfg(feature = "std")]
pub mod imp {
    use crate::{find_lib_error, Result};
    use core::str::FromStr;
    use dynamic_loader_cache::{Cache as LdCache, Result as LdResult};
    use spin::Lazy;
    use std::path::PathBuf;

    static LD_LIBRARY_PATH: Lazy<Box<[PathBuf]>> = Lazy::new(|| {
        let library_path = std::env::var("LD_LIBRARY_PATH").unwrap_or(String::new());
        deal_path(&library_path)
    });
    static DEFAULT_PATH: spin::Lazy<Box<[PathBuf]>> = Lazy::new(|| unsafe {
        vec![
            PathBuf::from_str("/lib").unwrap_unchecked(),
            PathBuf::from_str("/usr/lib").unwrap_unchecked(),
        ]
        .into_boxed_slice()
    });
    static LD_CACHE: Lazy<Box<[PathBuf]>> = Lazy::new(|| {
        build_ld_cache().unwrap_or_else(|err| {
            log::warn!("Build ld cache failed: {}", err);
            Box::new([])
        })
    });

    #[inline]
    fn build_ld_cache() -> LdResult<Box<[PathBuf]>> {
        use std::collections::HashSet;

        let cache = LdCache::load()?;
        let unique_ld_foders = cache
            .iter()?
            .filter_map(LdResult::ok)
            .map(|entry| {
                // Since the `full_path` is always a file, we can always unwrap it
                entry.full_path.parent().unwrap().to_owned()
            })
            .collect::<HashSet<_>>();
        Ok(Vec::from_iter(unique_ld_foders).into_boxed_slice())
    }

    #[inline]
    pub(crate) fn fixup_rpath(lib_path: &str, rpath: &str) -> Box<[PathBuf]> {
        if !rpath.contains('$') {
            return deal_path(rpath);
        }
        for s in rpath.split('$').skip(1) {
            if !s.starts_with("ORIGIN") && !s.starts_with("{ORIGIN}") {
                log::warn!("DT_RUNPATH format is incorrect: [{}]", rpath);
                return Box::new([]);
            }
        }
        let dir = if let Some((path, _)) = lib_path.rsplit_once('/') {
            path
        } else {
            "."
        };
        deal_path(&rpath.to_string().replace("$ORIGIN", dir))
    }

    #[inline]
    fn deal_path(s: &str) -> Box<[PathBuf]> {
        s.split(":")
            .map(|str| std::path::PathBuf::try_from(str).unwrap())
            .collect()
    }

    #[inline]
    pub(crate) fn find_library(
        cur_rpath: &Box<[PathBuf]>,
        lib_name: &str,
        mut f: impl FnMut(std::fs::File, &std::path::PathBuf) -> Result<()>,
    ) -> Result<()> {
        // Search order: DT_RPATH(deprecated) -> LD_LIBRARY_PATH -> DT_RUNPATH -> /etc/ld.so.cache -> /lib:/usr/lib.
        let search_paths = LD_LIBRARY_PATH
            .iter()
            .chain(cur_rpath.iter())
            .chain(LD_CACHE.iter())
            .chain(DEFAULT_PATH.iter());

        for path in search_paths {
            let file_path = path.join(lib_name);
            log::trace!("Try to open dependency shared object: [{:?}]", file_path);
            if let Ok(file) = std::fs::File::open(&file_path) {
                match f(file, &file_path) {
                    Ok(_) => return Ok(()),
                    Err(err) => {
                        log::debug!("Cannot load dylib: [{:?}] reason: [{:?}]", file_path, err)
                    }
                }
            }
        }
        Err(find_lib_error(format!("can not find file: {}", lib_name)))
    }
}

#[allow(unused_variables)]
/// It is the same as `dlopen`.
pub unsafe extern "C" fn dlopen(filename: *const c_char, flags: c_int) -> *const c_void {
    let mut lib = if filename.is_null() {
        MANAGER.read().all.get_index(0).unwrap().1.get_dylib()
    } else {
        #[cfg(feature = "std")]
        {
            let flags = OpenFlags::from_bits_retain(flags as _);
            let filename = core::ffi::CStr::from_ptr(filename);
            let path = filename.to_str().unwrap();
            if let Ok(lib) = ElfLibrary::dlopen(path, flags) {
                lib
            } else {
                return core::ptr::null();
            }
        }
        #[cfg(not(feature = "std"))]
        return core::ptr::null();
    };
    Arc::into_raw(core::mem::take(&mut lib.deps).unwrap()) as _
}
