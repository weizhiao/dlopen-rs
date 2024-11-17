mod builtin;
pub(crate) mod ehframe;
pub(crate) mod tls;

#[cfg(feature = "debug")]
use super::debug::DebugInfo;
use crate::Result;
use alloc::vec::Vec;
use builtin::BuiltinSymbol;
use core::fmt::Debug;
use ehframe::EhFrame;
use elf_loader::{object::ElfBinary, ElfDylib, Loader, RelocatedDylib};
use tls::ElfTls;

pub struct ElfLibrary {
    pub(crate) dylib: ElfDylib<ElfTls, EhFrame>,
}

impl Debug for ElfLibrary {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.dylib.fmt(f)
    }
}

impl ElfLibrary {
    #[cfg(feature = "debug")]
    fn add_debug_info(dylib: &mut ElfDylib<ElfTls, EhFrame>) {
        unsafe {
            let debug_info = DebugInfo::new(
                dylib.base(),
                dylib.cname().as_ptr(),
                dylib.dynamic() as usize,
            );
            dylib.user_data_mut().data_mut().push(Box::new(debug_info));
        }
    }

    #[cfg(feature = "std")]
    fn add_register_mark(dylib: &mut ElfDylib<ElfTls, EhFrame>) {
        use crate::register::RegisterInfo;
        let info = RegisterInfo::new(&dylib);
        unsafe {
            dylib.user_data_mut().data_mut().push(Box::new(info));
        }
    }

    /// Find and load a elf dynamic library from path.
    ///
    /// The `filename` argument may be either:
    ///
    /// * A library filename;
    /// * The absolute path to the library;
    /// * A relative (to the current working directory) path to the library.
    /// # Examples
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let lib = ELFLibrary::from_file("/path/to/awesome.module")
    ///		.unwrap();
    /// ```
    ///
    #[cfg(feature = "std")]
    pub fn from_file(path: impl AsRef<std::ffi::OsStr>) -> Result<Self> {
        use elf_loader::object;

        let path = path.as_ref();
        let file_name = path.to_str().unwrap();
        let file = object::ElfFile::new(file_name, std::fs::File::open(path)?);
        let loader = Loader::<_>::new(file);
        let mut dylib = loader.load_dylib()?;
        #[cfg(feature = "debug")]
        ElfLibrary::add_debug_info(&mut dylib);
        ElfLibrary::add_register_mark(&mut dylib);
        Ok(Self { dylib })
    }

    /// Creates a new `ELFLibrary` instance from an open file handle.
    /// # Examples
    /// ```
    /// let file = File::open("path_to_elf").unwrap();
    /// let lib = ELFLibrary::from_open_file(file, "my_elf_library").unwrap();
    /// ```
    #[cfg(feature = "std")]
    pub fn from_open_file(file: std::fs::File, name: impl AsRef<str>) -> Result<ElfLibrary> {
        use elf_loader::object;

        let file = object::ElfFile::new(name.as_ref(), file);
        let loader = Loader::<_>::new(file);
        #[allow(unused_mut)]
        let mut dylib = loader.load_dylib()?;
        #[cfg(feature = "debug")]
        ElfLibrary::add_debug_info(&mut dylib);
        Ok(Self { dylib })
    }

    /// load a elf dynamic library from bytes
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let path = Path::new("/path/to/awesome.module");
    /// let bytes = std::fs::read(path).unwrap();
    /// let lib = ELFLibrary::from_binary(&bytes).unwarp();
    /// ```
    pub fn from_binary(bytes: &[u8], name: impl AsRef<str>) -> Result<Self> {
        let file = ElfBinary::new(name.as_ref(), bytes);
        let loader = Loader::<_>::new(file);
        #[allow(unused_mut)]
        let mut dylib = loader.load_dylib()?;
        #[cfg(feature = "debug")]
        ElfLibrary::add_debug_info(&mut dylib);
        Ok(Self { dylib })
    }

    /// get the name of the dependent libraries
    pub fn needed_libs(&self) -> &Vec<&str> {
        self.dylib.needed_libs()
    }

    pub fn is_finished(&self) -> bool {
        self.dylib.is_finished()
    }

    pub fn name(&self) -> &str {
        self.dylib.name()
    }

    /// use internal libraries to relocate the current library
    /// # Examples
    ///
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let libc = ELFLibrary::load_self("libc").unwrap();
    /// let libgcc = ELFLibrary::load_self("libgcc").unwrap();
    /// let lib = ELFLibrary::from_file("/path/to/awesome.module")
    /// 	.unwrap()
    /// 	.relocate(&[libgcc, libc])
    ///     .finish()
    ///		.unwrap();
    /// ```
    ///
    pub fn relocate(self, libs: impl AsRef<[RelocatedDylib]>) -> Self {
        Self {
            dylib: self.dylib.relocate::<BuiltinSymbol>(libs),
        }
    }

    /// use internal libraries and function closure to relocate the current library
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// extern "C" fn mymalloc(size: size_t) -> *mut c_void {
    ///     println!("malloc:{}bytes", size);
    ///     unsafe { nix::libc::malloc(size) }
    /// }
    /// let libc = ELFLibrary::load_self("libc").unwrap();
    /// let libgcc = ELFLibrary::load_self("libgcc").unwrap();
    /// let lib = ELFLibrary::from_file("/path/to/awesome.module")
    /// 	.unwrap()
    /// 	.relocate_with(&[libc, libgcc], |name| {
    ///         if name == "malloc" {
    ///	             return Some(mymalloc as _);
    ///         } else {
    ///	             return None;
    ///         }
    ///     })
    ///     .finish()
    ///		.unwrap();
    /// ```
    /// # Note
    /// It will use function closure to relocate current lib firstly
    pub fn relocate_with<F>(self, libs: impl AsRef<[RelocatedDylib]>, func: F) -> Self
    where
        F: Fn(&str) -> Option<*const ()> + 'static,
    {
        Self {
            dylib: self.dylib.relocate_with::<BuiltinSymbol, _>(libs, func),
        }
    }

    pub fn finish(self) -> Result<RelocatedDylib> {
        Ok(self.dylib.finish()?)
    }
}
