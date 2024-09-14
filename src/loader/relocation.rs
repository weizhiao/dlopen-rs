use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::CStr;
use core::fmt::Debug;
use core::sync::atomic::AtomicBool;

use super::{arch::*, builtin::BUILTIN, dso::ELFLibrary};
use super::{ExternLibrary, RelocatedLibraryInner};
use crate::loader::dso::CommonElfData;
use crate::relocate_error;
use crate::RelocatedLibrary;
use crate::Result;
use alloc::boxed::Box;
use alloc::format;
use elf::abi::*;

#[allow(unused)]
pub(crate) struct InternalLib {
    common: CommonElfData,
    libs: Box<[RelocatedLibrary]>,
    user_data: Option<Box<dyn Fn(&str) -> Option<*const ()>>>,
}

impl Debug for InternalLib {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("InternalLib")
            .field("name", &self.common.name())
            .field("libs", &self.libs)
            .finish()
    }
}

impl Drop for InternalLib {
    fn drop(&mut self) {
        if let Some(fini) = self.common_data().fini_fn() {
            fini();
        }
        if let Some(fini_array) = self.common_data().fini_array_fn() {
            for fini in *fini_array {
                fini();
            }
        }
    }
}

impl InternalLib {
    pub(crate) fn new(
        common: CommonElfData,
        libs: Vec<RelocatedLibrary>,
        user_data: Option<Box<dyn Fn(&str) -> Option<*const ()> + 'static>>,
    ) -> InternalLib {
        InternalLib {
            common,
            libs: libs.into_boxed_slice(),
            user_data,
        }
    }

    pub(crate) fn get_sym(&self, name: &str) -> Option<&ELFSymbol> {
        self.common.get_sym(name)
    }

    pub(crate) fn common_data(&self) -> &CommonElfData {
        &self.common
    }

    pub(crate) fn name(&self) -> &CStr {
        self.common.name()
    }
}

struct InternalSym<'temp> {
    sym: &'temp ELFSymbol,
    dso: &'temp CommonElfData,
}

#[allow(unused)]
enum SymDef<'temp> {
    Internal(InternalSym<'temp>),
    External(*const ()),
}

impl ELFLibrary {
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
    ///		.unwrap();
    /// ```
    ///
    pub fn relocate(self, libs: impl Into<Vec<RelocatedLibrary>>) -> Result<RelocatedLibrary> {
        self.relocate_impl(libs.into(), None)
    }

    /// Use both internal and external libraries to relocate the current library
    /// # Examples
    /// ```
    ///#[derive(Debug, Clone)]
    ///struct MyLib(Arc<Library>);
    ///impl ExternLibrary for MyLib {
    ///    fn get_sym(&self, name: &str) -> Option<*const ()> {
    ///        let sym: Option<*const ()> = unsafe {
    ///            self.0
    ///                .get::<*const usize>(name.as_bytes())
    ///                .map_or(None, |sym| Some(sym.into_raw().into_raw() as _))
    ///        };
    ///        sym
    ///    }
    ///}
    ///let libc = MyLib(Arc::new(unsafe {
    ///    Library::new("/lib/x86_64-linux-gnu/libc.so.6").unwrap()
    ///}));
    ///let libgcc = MyLib(Arc::new(unsafe {
    ///    Library::new("/lib/x86_64-linux-gnu/libgcc_s.so.1").unwrap()
    ///}));
    ///let libexample = ELFLibrary::from_file("/path/to/awesome.module")
    ///    .unwrap()
    ///    .relocate_with(&[], vec![libc, libgcc])
    ///    .unwrap();
    /// ```
    /// # Note
    /// It will use extern_libs to relocate current lib firstly
    pub fn relocate_with<T>(
        self,
        libs: impl Into<Vec<RelocatedLibrary>>,
        extern_libs: Vec<T>,
    ) -> Result<RelocatedLibrary>
    where
        T: ExternLibrary + 'static,
    {
        let func = move |name: &str| {
            let mut symbol = None;
            for lib in extern_libs.iter() {
                if let Some(sym) = lib.get_sym(name) {
                    symbol = Some(sym);
                    break;
                }
            }
            symbol
        };
        self.relocate_impl(libs.into(), Some(Box::new(func)))
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
    /// 	.relocate_with_func(&[libc, libgcc], |name| {
    ///         if name == "malloc" {
    ///	             return Some(mymalloc as _);
    ///         } else {
    ///	             return None;
    ///         }
    ///     })
    ///		.unwrap();
    /// ```
    /// # Note
    /// It will use function closure to relocate current lib firstly
    pub fn relocate_with_func<F>(
        self,
        libs: impl Into<Vec<RelocatedLibrary>>,
        func: F,
    ) -> Result<RelocatedLibrary>
    where
        F: Fn(&str) -> Option<*const ()> + 'static,
    {
        let func = move |name: &str| func(name);
        self.relocate_impl(libs.into(), Some(Box::new(func)))
    }

    fn relocate_impl(
        self,
        libs: Vec<RelocatedLibrary>,
        func: Option<Box<dyn Fn(&str) -> Option<*const ()> + 'static>>,
    ) -> Result<RelocatedLibrary> {
        let iter = self
            .relocation()
            .pltrel
            .unwrap_or(&[])
            .iter()
            .chain(self.relocation().rel.unwrap_or(&[]));

        let write_val = |addr: usize, symbol: usize| {
            unsafe {
                let rel_addr = (self.common_data().base() + addr) as *mut usize;
                rel_addr.write(symbol)
            };
        };

        let find_symbol = |name: &str, symdef: Option<SymDef<'_>>| -> Result<*const ()> {
            BUILTIN
                .get(name)
                .copied()
                .or_else(|| {
                    if let Some(f) = func.as_ref() {
                        return f(name);
                    }
                    None
                })
                .or_else(|| {
                    symdef.map(|symdef| match symdef {
                        SymDef::Internal(sym) => (sym.dso.base() + sym.sym.st_value as usize) as _,
                        SymDef::External(sym) => sym,
                    })
                })
                .ok_or(relocate_error(format!("can not relocate symbol {}", name)))
        };

        for rela in iter {
            let r_type = rela.r_info as usize & REL_MASK;
            let r_sym = rela.r_info as usize >> REL_BIT;
            let mut str_name = None;
            let symdef = if r_sym != 0 {
                let dynsym = unsafe { &*self.symtab().add(r_sym) };
                let temp_cstr_name = self
                    .strtab()
                    .get_cstr(dynsym.st_name as usize)
                    .map_err(relocate_error)?;
                let temp_str_name = temp_cstr_name.to_str().unwrap();
                str_name = Some(temp_str_name);
                if dynsym.st_shndx != SHN_UNDEF {
                    Some(SymDef::Internal(InternalSym {
                        sym: dynsym,
                        dso: self.common_data(),
                    }))
                } else {
                    let mut symbol = None;
                    for lib in libs.iter() {
                        match &lib.inner.as_ref().1 {
                            RelocatedLibraryInner::Internal(lib) => {
                                if let Some(sym) = lib.get_sym(temp_str_name) {
                                    symbol = Some(SymDef::Internal(InternalSym {
                                        sym,
                                        dso: lib.common_data(),
                                    }));
                                    break;
                                }
                            }
                            #[cfg(feature = "ldso")]
                            RelocatedLibraryInner::External(lib) => {
                                if let Some(sym) = lib.get_sym(temp_cstr_name) {
                                    symbol = Some(SymDef::External(sym));
                                    break;
                                }
                            }
                        }
                    }
                    symbol
                }
            } else {
                None
            };

            /*
                A Represents the addend used to compute the value of the relocatable field.
                B Represents the base address at which a shared object has been loaded into memory during execution.
                S Represents the value of the symbol whose index resides in the relocation entry.
            */

            match r_type as _ {
                // do nothing
                REL_NONE => {}
                // REL_GOT/REL_JUMP_SLOT: S  REL_SYMBOLIC: S + A
                REL_JUMP_SLOT | REL_GOT | REL_SYMBOLIC => {
                    let symbol = find_symbol(str_name.unwrap(), symdef)?;
                    write_val(
                        rela.r_offset as usize,
                        symbol as usize + rela.r_addend as usize,
                    );
                }
                // B + A
                REL_RELATIVE => {
                    write_val(
                        rela.r_offset as usize,
                        self.common_data().base() + rela.r_addend as usize,
                    );
                }
                // ELFTLS
                #[cfg(feature = "tls")]
                REL_DTPMOD => {
                    if r_sym != 0 {
                        let symdef = symdef.ok_or(relocate_error(format!(
                            "can not relocate symbol {}",
                            str_name.unwrap()
                        )))?;
                        match symdef {
                            SymDef::Internal(def) => {
                                write_val(rela.r_offset as usize, def.dso.tls().unwrap() as usize)
                            }
                            SymDef::External(_) => {
                                return Err(relocate_error(format!(
                                    "can not relocate symbol {}",
                                    str_name.unwrap()
                                )));
                            }
                        }
                    } else {
                        write_val(
                            rela.r_offset as usize,
                            self.common_data().tls().unwrap() as usize,
                        );
                    }
                }
                #[cfg(feature = "tls")]
                REL_DTPOFF => {
                    let symdef = if let SymDef::Internal(def) = symdef.ok_or(relocate_error(
                        format!("can not relocate symbol {}", str_name.unwrap()),
                    ))? {
                        def
                    } else {
                        return Err(relocate_error(format!(
                            "can not relocate symbol {}",
                            str_name.unwrap()
                        )));
                    };
                    // offset in tls
                    let tls_val = (symdef.sym.st_value as usize + rela.r_addend as usize)
                        .wrapping_sub(TLS_DTV_OFFSET);
                    write_val(rela.r_offset as usize, tls_val);
                }
                _ => {
                    // REL_TPOFF：这种类型的重定位明显做不到，它是为静态模型设计的，这种方式
                    // 可以通过带偏移量的内存读取来获取TLS变量，无需使用__tls_get_addr，
                    // 实现它需要对要libc做修改，因为它要使用tp来访问thread local，
                    // 而线程栈里保存的东西完全是由libc控制的

                    return Err(relocate_error(format!(
                        "unsupport relocate type {}",
                        r_type
                    )));
                }
            }
        }

        if let Some(init) = self.init_fn() {
            init();
        }

        if let Some(init_array) = self.init_array_fn() {
            for init in *init_array {
                init();
            }
        }

        if let Some(relro) = &self.relro() {
            relro.relro()?;
        }
        #[cfg(feature = "debug")]
        unsafe {
            super::debug::dl_debug_finish();
        }
        let common = self.into_common_data();

        let internal_lib = InternalLib::new(common, libs, func);
        Ok(RelocatedLibrary {
            inner: Arc::new((
                AtomicBool::new(false),
                super::RelocatedLibraryInner::Internal(internal_lib),
            )),
        })
    }
}
