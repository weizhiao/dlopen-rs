use crate::{loader::ElfLibrary, Result};
use elf_loader::RelocatedDylib;
use hashbrown::HashMap;
use std::{env, fs::File, path::PathBuf, sync::OnceLock};

static LD_LIBRARY_PATH: OnceLock<Vec<PathBuf>> = OnceLock::new();
#[cfg(feature = "ldso")]
const SYS_LIBS: [&'static str; 4] = ["libc", "libgcc_s", "libstdc++", "libm"];

impl ElfLibrary {
    /// Load a shared library from a specified path
    ///
    /// # Note
    /// Please set the `RUST_LD_LIBRARY_PATH` environment variable before calling this function.
    /// dlopen-rs will look for dependent dynamic libraries in the paths saved in `RUST_LD_LIBRARY_PATH`.
    /// The way to set `RUST_LD_LIBRARY_PATH` is the same as `LD_LIBRARY_PATH`.
    ///
    /// # Example
    /// ```no_run
    /// use std::path::Path;
    /// use dlopen_rs::ELFLibrary;
    ///
    /// let path = Path::new("/path/to/library.so");
    /// let lib = ELFLibrary::dlopen(path).expect("Failed to load library");
    /// ```
    pub fn dlopen(path: impl AsRef<std::ffi::OsStr>) -> Result<RelocatedDylib> {
        LD_LIBRARY_PATH.get_or_init(|| {
            let ld_library_path =
                env::var("RUST_LD_LIBRARY_PATH").expect("env RUST_LD_LIBRARY_PATH");

            ld_library_path
                .split(":")
                .map(|str| PathBuf::try_from(str).unwrap())
                .collect()
        });
        let lib = ElfLibrary::from_file(path)?;
        let mut relocated_libs = HashMap::new();
        // 不支持循环依赖，relocated_libs的作用是防止一个库被多次重复加载
        fn load_and_relocate(
            lib: ElfLibrary,
            relocated_libs: &mut HashMap<String, RelocatedDylib>,
        ) -> Result<RelocatedDylib> {
            let needed_libs_name = lib.needed_libs();
            let mut needed_libs = vec![];
            for needed_lib_name in needed_libs_name {
                // 不需要加载其他动态链接器
                if needed_lib_name.contains("ld-") {
                    continue;
                }

                let find_needed_lib = |relocated_libs: &mut HashMap<String, RelocatedDylib>| {
                    // 像libc这种系统库需要使用系统的动态链接器加载，前提是开启ldso feature
                    #[cfg(feature = "ldso")]
                    for sys_lib_name in SYS_LIBS {
                        if needed_lib_name.contains(sys_lib_name) {
                            let lib = ElfLibrary::sys_load(needed_lib_name)?;
                            relocated_libs
                                .insert_unique_unchecked(needed_lib_name.to_string(), lib.clone());
                            return Ok(lib);
                        }
                    }

                    // 按RUST_LD_LIBRARY_PATH指定的路径依次寻找
                    for sys_path in LD_LIBRARY_PATH.get().unwrap() {
                        let file_path = sys_path.join(needed_lib_name);
                        match File::open(&file_path) {
                            Ok(file) => {
                                let new_lib =
                                    ElfLibrary::from_open_file(file, file_path.to_str().unwrap())?;
                                let lib = load_and_relocate(new_lib, relocated_libs)?;
                                relocated_libs.insert_unique_unchecked(
                                    needed_lib_name.to_string(),
                                    lib.clone(),
                                );
                                return Ok(lib);
                            }
                            Err(_) => continue,
                        }
                    }
                    Err(crate::Error::IOError {
                        err: std::io::Error::new(
                            std::io::ErrorKind::NotFound,
                            format!("can not find file: {}", needed_lib_name),
                        ),
                    })
                };

                let needed_lib = relocated_libs
                    .get(*needed_lib_name)
                    .cloned()
                    .unwrap_or(find_needed_lib(relocated_libs)?);

                needed_libs.push(needed_lib);
            }
            lib.relocate(needed_libs).finish()
        }
        load_and_relocate(lib, &mut relocated_libs)
    }
}
