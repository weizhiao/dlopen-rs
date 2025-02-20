pub(crate) mod builtin;
pub(crate) mod ehframe;
pub(crate) mod tls;

#[cfg(feature = "debug")]
use super::debug::DebugInfo;
use crate::{
    find_lib_error, find_symbol_error,
    register::{register, DylibState, MANAGER},
    OpenFlags, Result,
};
use alloc::{boxed::Box, format, sync::Arc, vec::Vec};
use core::{any::Any, ffi::CStr, fmt::Debug};
use ehframe::EhFrame;
use elf_loader::{
    abi::PT_GNU_EH_FRAME,
    arch::{ElfPhdr, ElfRela, Phdr},
    mmap::MmapImpl,
    object::{ElfBinary, ElfObject},
    segment::ElfSegments,
    CoreComponent, CoreComponentRef, ElfDylib, Loader, RelocatedDylib, Symbol, UserData,
};

pub(crate) const EH_FRAME_ID: u8 = 0;
#[cfg(feature = "debug")]
pub(crate) const DEBUG_INFO_ID: u8 = 1;
#[cfg(feature = "tls")]
const TLS_ID: u8 = 2;
const CLOSURE: u8 = 3;

#[inline]
pub(crate) fn find_symbol<'lib, T>(
    libs: &'lib [RelocatedDylib<'static>],
    name: &str,
) -> Result<Symbol<'lib, T>> {
    log::info!("Get the symbol [{}] in [{}]", name, libs[0].shortname());
    libs.iter()
        .find_map(|lib| unsafe { lib.get::<T>(name) })
        .ok_or(find_symbol_error(format!("can not find symbol:{}", name)))
}

/// An unrelocated dynamic library
pub struct ElfLibrary {
    pub(crate) dylib: ElfDylib,
    pub(crate) flags: OpenFlags,
}

impl Debug for ElfLibrary {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.dylib.fmt(f)
    }
}

#[inline(always)]
#[allow(unused)]
fn parse_phdr(
    cname: &CStr,
    phdr: &Phdr,
    segments: &ElfSegments,
    data: &mut UserData,
) -> core::result::Result<(), Box<dyn Any>> {
    match phdr.p_type {
        PT_GNU_EH_FRAME => {
            data.insert(
                EH_FRAME_ID,
                Box::new(EhFrame::new(
                    phdr,
                    segments.base()..segments.base() + segments.len(),
                )),
            );
        }
        #[cfg(feature = "debug")]
        elf_loader::abi::PT_DYNAMIC => {
            data.insert(
                DEBUG_INFO_ID,
                Box::new(unsafe {
                    DebugInfo::new(
                        segments.base(),
                        cname.as_ptr() as _,
                        segments.base() + phdr.p_vaddr as usize,
                    )
                }),
            );
        }
        #[cfg(feature = "tls")]
        elf_loader::abi::PT_TLS => {
            data.insert(TLS_ID, Box::new(tls::ElfTls::new(phdr, segments.base())));
        }
        _ => {}
    }
    Ok(())
}

#[inline(always)]
#[allow(unused)]
pub(crate) fn deal_unknown<'scope>(
    rela: &ElfRela,
    lib: &CoreComponent,
    mut deps: impl Iterator<Item = &'scope RelocatedDylib<'static>> + Clone,
) -> core::result::Result<(), Box<dyn Any>> {
    #[cfg(feature = "tls")]
    match rela.r_type() as _ {
        elf_loader::arch::REL_DTPMOD => {
            let r_sym = rela.r_symbol();
            let r_off = rela.r_offset();
            let ptr = (lib.base() + r_off) as *mut usize;
            let cast = |core: &elf_loader::CoreComponent| unsafe {
                core.user_data()
                    .get(TLS_ID)
                    .unwrap()
                    .downcast_ref::<tls::ElfTls>()
                    .unwrap_unchecked()
                    .module_id()
            };
            if r_sym != 0 {
                let (dynsym, syminfo) = lib.symtab().unwrap().symbol_idx(r_sym);
                if dynsym.is_local() {
                    unsafe { ptr.write(cast(lib)) };
                    return Ok(());
                } else {
                    if let Some(id) = deps.find_map(|lib| unsafe {
                        lib.symtab()
                            .lookup_filter(&syminfo)
                            .map(|_| cast(lib.core_component_ref()))
                    }) {
                        unsafe { ptr.write(id) };
                        return Ok(());
                    };
                };
            } else {
                unsafe { ptr.write(cast(lib)) };
                return Ok(());
            }
        }
        _ => {}
    }
    log::error!("Relocating dylib [{}] failed!", lib.name());
    Err(Box::new(()))
}

#[inline]
pub(crate) fn create_lazy_scope(
    deps: &[RelocatedDylib],
    is_lazy: bool,
) -> Option<Box<dyn for<'a> Fn(&'a str) -> Option<*const ()>>> {
    if is_lazy {
        let deps_weak: Vec<CoreComponentRef> = deps
            .iter()
            .map(|dep| unsafe { dep.core_component_ref().downgrade() })
            .collect();
        Some(Box::new(move |name: &str| {
            deps_weak.iter().find_map(|dep| unsafe {
                RelocatedDylib::from_core_component(dep.upgrade().unwrap())
                    .get::<()>(name)
                    .map(|sym| sym.into_raw())
            })
        })
            as Box<dyn Fn(&str) -> Option<*const ()> + 'static>)
    } else {
        None
    }
}

fn from_impl(object: impl ElfObject, flags: OpenFlags) -> Result<ElfLibrary> {
    let mut loader = Loader::<MmapImpl>::new();
    #[cfg(feature = "std")]
    unsafe {
        loader.set_init_params(
            crate::init::ARGC,
            (*core::ptr::addr_of!(crate::init::ARGV)).as_ptr() as usize,
            crate::init::ENVP,
        )
    };
    let lazy_bind = if flags.contains(OpenFlags::RTLD_LAZY) {
        Some(true)
    } else if flags.contains(OpenFlags::RTLD_NOW) {
        Some(false)
    } else {
        None
    };
    let dylib = loader.load_dylib(object, lazy_bind, parse_phdr)?;
    log::debug!(
        "Loading dylib [{}] at address [0x{:x}-0x{:x}]",
        dylib.name(),
        dylib.base(),
        dylib.base() + dylib.map_len()
    );
    let lib = ElfLibrary { dylib, flags };
    Ok(lib)
}

impl ElfLibrary {
    /// Find and load a elf dynamic library from path.
    ///
    /// The `path` argument may be either:
    /// * The absolute path to the library;
    /// * A relative (to the current working directory) path to the library.   
    ///
    /// The `flags` argument can control how dynamic libraries are loaded.
    ///
    /// # Examples
    /// ```no_run
    /// # use ::dlopen_rs::ElfLibrary;
    /// let lib = ElfLibrary::from_file("/path/to/awesome.module", OpenFlags::RTLD_LOCAL)
    ///		.unwrap();
    /// ```
    ///
    #[cfg(feature = "std")]
    #[inline]
    pub fn from_file(path: impl AsRef<std::ffi::OsStr>, flags: OpenFlags) -> Result<Self> {
        let path = path.as_ref().to_str().unwrap();
        let file = std::fs::File::open(path)?;
        Self::from_open_file(file, path, flags)
    }

    /// Creates a new `ElfLibrary` instance from an open file handle.
    /// The `flags` argument can control how dynamic libraries are loaded.
    /// # Examples
    /// ```
    /// let file = File::open("/path/to/awesome.module").unwrap();
    /// let lib = ElfLibrary::from_open_file(file, "/path/to/awesome.module", OpenFlags::RTLD_LOCAL).unwrap();
    /// ```
    #[cfg(feature = "std")]
    #[inline]
    pub fn from_open_file(
        file: std::fs::File,
        path: impl AsRef<str>,
        flags: OpenFlags,
    ) -> Result<ElfLibrary> {
        use std::os::fd::IntoRawFd;

        use elf_loader::object;
        let file = unsafe { object::ElfFile::from_owned_fd(path.as_ref(), file.into_raw_fd()) };
        from_impl(file, flags)
    }

    /// Load a elf dynamic library from bytes.
    /// The `flags` argument can control how dynamic libraries are loaded.
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ElfLibrary;
    /// let path = Path::new("/path/to/awesome.module");
    /// let bytes = std::fs::read(path).unwrap();
    /// let lib = ElfLibrary::from_binary(&bytes, "/path/to/awesome.module", OpenFlags::RTLD_LOCAL).unwarp();
    /// ```
    #[inline]
    pub fn from_binary(
        bytes: impl AsRef<[u8]>,
        path: impl AsRef<str>,
        flags: OpenFlags,
    ) -> Result<Self> {
        let file = ElfBinary::new(path.as_ref(), bytes.as_ref());
        from_impl(file, flags)
    }

    /// Load an existing dynamic library using the shortname of the library
    /// # Examples
    /// ```no_run
    /// # use ::dlopen_rs::ElfLibrary;
    /// let libc = ElfLibrary::load_existing("libc.so.6").unwrap();
    /// ```
    pub fn load_existing(shortname: &str) -> Result<Dylib> {
        MANAGER
            .read()
            .all
            .get(shortname)
            .filter(|lib| lib.deps().is_some())
            .map(|lib| lib.get_dylib())
            .ok_or(find_lib_error(format!("{}: load fail", shortname)))
    }

    /// Gets the name of the dependent libraries
    pub fn needed_libs(&self) -> &[&str] {
        self.dylib.needed_libs()
    }

    /// Gets the name of the dynamic library.
    pub fn name(&self) -> &str {
        self.dylib.name()
    }

    fn relocate_impl<F>(self, libs: &[Dylib], find: &F) -> Result<Dylib>
    where
        F: for<'b> Fn(&'b str) -> Option<*const ()>,
    {
        let mut deps = Vec::new();
        deps.push(unsafe { RelocatedDylib::from_core_component(self.dylib.core_component()) });
        deps.extend(libs.iter().map(|lib| lib.inner.clone()));
        let deps = Arc::new(deps.into_boxed_slice());
        let lazy_scope = create_lazy_scope(&deps, self.dylib.is_lazy());
        let cur_lib: RelocatedDylib<'static> = unsafe {
            core::mem::transmute(self.dylib.relocate(
                deps.iter(),
                find,
                deal_unknown,
                lazy_scope,
            )?)
        };
        if !self.flags.contains(OpenFlags::CUSTOM_NOT_REGISTER) {
            register(
                cur_lib.clone(),
                self.flags,
                Some(deps.clone()),
                &mut MANAGER.write(),
                *DylibState::default().set_relocated(),
            );
            Ok(Dylib {
                inner: cur_lib,
                flags: self.flags,
                deps: Some(deps),
            })
        } else {
            Ok(Dylib {
                inner: cur_lib,
                flags: self.flags,
                deps: Some(deps),
            })
        }
    }

    /// Use libraries to relocate the current library.
    /// # Examples
    /// ```no_run
    /// # use ::dlopen_rs::ElfLibrary;
    /// let libc = ElfLibrary::load_existing("libc").unwrap();
    /// let libgcc = ElfLibrary::load_existing("libgcc").unwrap();
    /// let lib = ElfLibrary::from_file("/path/to/awesome.module", OpenFlags::RTLD_LOCAL)
    /// 	.unwrap()
    /// 	.relocate(&[libgcc, libc]);
    /// ```
    #[inline]
    pub fn relocate(self, libs: impl AsRef<[Dylib]>) -> Result<Dylib> {
        self.relocate_impl(libs.as_ref(), &|name| builtin::BUILTIN.get(name).copied())
    }

    /// Use libraries and function closure to relocate the current library.
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ElfLibrary;
    /// extern "C" fn mymalloc(size: size_t) -> *mut c_void {
    ///     println!("malloc:{}bytes", size);
    ///     unsafe { libc::malloc(size) }
    /// }
    /// let libc = ElfLibrary::load_existing("libc").unwrap();
    /// let libgcc = ElfLibrary::load_existing("libgcc").unwrap();
    /// let lib = ElfLibrary::from_file("/path/to/awesome.module", OpenFlags::RTLD_LOCAL)
    /// 	.unwrap()
    /// 	.relocate_with(&[libc, libgcc], &|name| {
    ///         if name == "malloc" {
    ///	             return Some(mymalloc as _);
    ///         } else {
    ///	             return None;
    ///         }
    ///     })
    ///		.unwrap();
    /// ```
    /// # Note
    /// It will use function closure to relocate current lib firstly.
    #[inline]
    pub fn relocate_with<F>(mut self, libs: impl AsRef<[Dylib]>, func: F) -> Result<Dylib>
    where
        F: for<'b> Fn(&'b str) -> Option<*const ()> + 'static,
    {
        type Closure = Box<dyn Fn(&str) -> Option<*const ()> + 'static>;

        self.dylib.user_data_mut().unwrap().insert(
            CLOSURE,
            Box::new(
                Box::new(move |name: &str| func(name).or(builtin::BUILTIN.get(name).copied()))
                    as Closure,
            ),
        );
        let func_ref: &Closure = unsafe {
            core::mem::transmute(
                self.dylib
                    .user_data()
                    .get(CLOSURE)
                    .unwrap()
                    .downcast_ref::<Closure>()
                    .unwrap(),
            )
        };
        self.relocate_impl(libs.as_ref(), func_ref)
    }
}

/// An relocated dynamic library
#[derive(Clone)]
pub struct Dylib {
    pub(crate) inner: RelocatedDylib<'static>,
    pub(crate) flags: OpenFlags,
    pub(crate) deps: Option<Arc<Box<[RelocatedDylib<'static>]>>>,
}

impl Debug for Dylib {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Dylib")
            .field("inner", &self.inner)
            .field("flags", &self.flags)
            .finish()
    }
}

impl Dylib {
    /// Get the name of the dynamic library.
    #[inline]
    pub fn name(&self) -> &str {
        self.inner.name()
    }

    /// Get the C-style name of the dynamic library.
    #[inline]
    pub fn cname(&self) -> &CStr {
        self.inner.cname()
    }

    /// Get the base address of the dynamic library.
    #[inline]
    pub fn base(&self) -> usize {
        self.inner.base()
    }

    /// Get the program headers of the dynamic library.
    #[inline]
    pub fn phdrs(&self) -> &[ElfPhdr] {
        self.inner.phdrs()
    }

    /// Get the needed libs' name of the elf object.
    #[inline]
    pub fn needed_libs(&self) -> &[&str] {
        self.inner.needed_libs()
    }

    /// Get a pointer to a function or static variable by symbol name.
    ///
    /// The symbol is interpreted as-is; no mangling is done. This means that symbols like `x::y` are
    /// most likely invalid.
    ///
    /// # Safety
    /// Users of this API must specify the correct type of the function or variable loaded.
    ///
    /// # Examples
    /// ```no_run
    /// unsafe {
    ///     let awesome_function: Symbol<unsafe extern fn(f64) -> f64> =
    ///         lib.get("awesome_function").unwrap();
    ///     awesome_function(0.42);
    /// }
    /// ```
    /// A static variable may also be loaded and inspected:
    /// ```no_run
    /// unsafe {
    ///     let awesome_variable: Symbol<*mut f64> = lib.get("awesome_variable").unwrap();
    ///     **awesome_variable = 42.0;
    /// };
    /// ```
    #[inline]
    pub unsafe fn get<'lib, T>(&'lib self, name: &str) -> Result<Symbol<'lib, T>> {
        find_symbol(self.deps.as_ref().unwrap(), name)
    }

    /// Load a versioned symbol from the dynamic library.
    ///
    /// # Examples
    /// ```
    /// let symbol = unsafe { lib.get_version::<fn()>>("function_name", "1.0").unwrap() };
    /// ```
    #[cfg(feature = "version")]
    #[inline]
    pub unsafe fn get_version<'lib, T>(
        &'lib self,
        name: &str,
        version: &str,
    ) -> Result<Symbol<'lib, T>> {
        self.inner
            .get_version(name, version)
            .ok_or(find_symbol_error(format!("can not find symbol:{}", name)))
    }
}
