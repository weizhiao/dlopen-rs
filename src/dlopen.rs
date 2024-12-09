use crate::{find_lib_error, loader::ElfLibrary, register::REGISTER_LIBS, Result};
use alloc::{format, vec::Vec};
use elf_loader::RelocatedDylib;
use spin::Mutex;

static LOCK: Mutex<()> = Mutex::new(());
#[cfg(feature = "std")]
static LD_LIBRARY_PATH: std::sync::OnceLock<Vec<std::path::PathBuf>> = std::sync::OnceLock::new();
#[cfg(feature = "std")]
static LAZY_BIND: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
#[cfg(feature = "ldso")]
const SYS_LIBS: [&'static str; 4] = ["libc.so", "libgcc_s.so", "libstdc++", "libm"];

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
    #[cfg(feature = "std")]
    pub fn dlopen(path: impl AsRef<std::ffi::OsStr>) -> Result<RelocatedDylib> {
        let lazy_bind = LAZY_BIND.get_or_init(|| {
            std::env::var("LD_BIND_NOW")
                .ok()
                .map(|val| {
                    val.parse::<u8>()
                        .expect("set LD_BIND_NOW = 0 or set LD_BIND_NOW = 1")
                })
                .map(|val| val == 0)
        });
        // 检查是否是已经加载的库
        if let Some(Some(lib)) = REGISTER_LIBS
            .read()
            .get(path.as_ref().to_str().unwrap())
            .map(|lib| lib.inner.upgrade())
        {
            return Ok(RelocatedDylib { inner: lib });
        };

        let lock = LOCK.lock();
        let lib = ElfLibrary::from_file(path, lazy_bind.clone())?
            .register()
            .dlopen_impl()?;
        drop(lock);
        Ok(lib)
    }

    /// Load a shared library from bytes
    /// # Note
    /// Please set the `LD_LIBRARY_PATH` environment variable before calling this function.
    /// dlopen-rs will look for dependent dynamic libraries in the paths saved in `LD_LIBRARY_PATH`.
    /// In addition, dlopen-rs also looks for dependent dynamic libraries in the registered dynamic library
    pub fn dlopen_from_binary(bytes: &[u8], name: impl AsRef<str>) -> Result<RelocatedDylib> {
        #[cfg(feature = "std")]
        let lazy_bind = LAZY_BIND.get_or_init(|| {
            std::env::var("LD_BIND_NOW")
                .ok()
                .map(|val| {
                    val.parse::<u8>()
                        .expect("set LD_BIND_NOW = 0 or set LD_BIND_NOW = 1")
                })
                .map(|val| val == 0)
        });
        #[cfg(not(feature = "std"))]
        let lazy_bind = None;
        // 检查是否是已经加载的库
        if let Some(Some(lib)) = REGISTER_LIBS
            .read()
            .get(name.as_ref())
            .map(|lib| lib.inner.upgrade())
        {
            return Ok(RelocatedDylib { inner: lib });
        };
        let lock = LOCK.lock();
        let lib = ElfLibrary::from_binary(bytes, name, lazy_bind.clone())?
            .register()
            .dlopen_impl()?;
        drop(lock);
        Ok(lib)
    }

    fn dlopen_impl(self) -> Result<RelocatedDylib> {
        #[cfg(feature = "std")]
        LD_LIBRARY_PATH.get_or_init(|| {
            let ld_library_path = std::env::var("LD_LIBRARY_PATH").expect("env LD_LIBRARY_PATH");
            ld_library_path
                .split(":")
                .map(|str| std::path::PathBuf::try_from(str).unwrap())
                .collect()
        });
        let needed_libs_name = self.needed_libs();
        let mut needed_libs = Vec::new();
        for needed_lib_name in needed_libs_name {
            // 不需要加载其他动态链接器
            if needed_lib_name.contains("ld-") {
                continue;
            }

            let read_lock = REGISTER_LIBS.read();
            if let Some(Some(needed_lib)) = read_lock
                .get(*needed_lib_name)
                .map(|lib| lib.inner.upgrade())
            {
                needed_libs.push(RelocatedDylib { inner: needed_lib });
            } else {
                #[cfg(feature = "std")]
                let needed_lib = {
                    drop(read_lock);
                    let mut needed_lib = None;
                    // 像libc这种系统库需要使用系统的动态链接器加载，前提是开启ldso feature
                    #[cfg(feature = "ldso")]
                    for sys_lib_name in SYS_LIBS {
                        if needed_lib_name.contains(sys_lib_name) {
                            needed_lib = Some(ElfLibrary::sys_load(needed_lib_name)?);
                            break;
                        }
                    }

                    if needed_lib.is_none() {
                        // 按LD_LIBRARY_PATH指定的路径依次寻找
                        for sys_path in LD_LIBRARY_PATH.get().unwrap() {
                            let file_path = sys_path.join(needed_lib_name);
                            match std::fs::File::open(&file_path) {
                                Ok(file) => {
                                    let new_lib = ElfLibrary::from_open_file(
                                        file,
                                        file_path.to_str().unwrap(),
                                        None,
                                    )?
                                    .register();
                                    needed_lib = Some(Self::dlopen_impl(new_lib)?);
                                }
                                Err(_) => continue,
                            }
                        }
                    }
                    needed_lib
                };
                #[cfg(not(feature = "std"))]
                let needed_lib = None;
                if let Some(needed_lib) = needed_lib {
                    needed_libs.push(needed_lib);
                } else {
                    return Err(find_lib_error(format!(
                        "can not find file: {}",
                        needed_lib_name
                    )));
                }
            }
        }
        self.relocate(needed_libs).finish()
    }
}
