use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicBool;

use super::ExternLibrary;
use super::{arch::*, builtin::BUILTIN, dso::ELFLibrary};
use crate::relocate_error;
use crate::RelocatedLibrary;
use crate::Result;
use alloc::boxed::Box;
use alloc::format;
use elf::abi::*;

#[allow(unused)]
struct SymDef<'temp> {
    sym: &'temp ELFSymbol,
    base: usize,
    #[cfg(feature = "tls")]
    tls: Option<usize>,
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
        mut self,
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
                let rel_addr = (self.extra_data().base() + addr) as *mut usize;
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
                    symdef.map(|symdef| {
                        if symdef.sym.st_info & 0xf != STT_GNU_IFUNC {
                            (symdef.base + symdef.sym.st_value as usize) as _
                        } else {
                            let ifunc: fn() -> usize = unsafe {
                                core::mem::transmute(symdef.base + symdef.sym.st_value as usize)
                            };
                            ifunc() as _
                        }
                    })
                })
                .ok_or(relocate_error(format!(
                    "{}: can not find symbol {} in dependency libraries",
                    self.name(),
                    name
                )))
        };

        for rela in iter {
            let r_type = rela.r_info as usize & REL_MASK;
            let r_sym = rela.r_info as usize >> REL_BIT;
            let mut str_name = None;
            let symdef = if r_sym != 0 {
                let (dynsym, syminfo) = self.symbols().rel_symbol(r_sym);
                str_name = Some(syminfo.name);
                if dynsym.st_shndx != SHN_UNDEF {
                    Some(SymDef {
                        sym: dynsym,
                        base: self.extra_data().base(),
                        #[cfg(feature = "tls")]
                        tls: self.extra_data().tls().map(|tls| tls as usize),
                    })
                } else {
                    let mut symbol = None;
                    for lib in libs.iter() {
                        if let Some(sym) = lib.inner().symbols().get_sym(&syminfo) {
                            symbol = Some(SymDef {
                                sym,
                                base: lib.base(),
                                #[cfg(feature = "tls")]
                                tls: lib.inner().tls(),
                            });
                            break;
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
                        self.extra_data().base() + rela.r_addend as usize,
                    );
                }
                // ELFTLS
                #[cfg(feature = "tls")]
                REL_DTPMOD => {
                    if r_sym != 0 {
                        let symdef = symdef.ok_or(relocate_error(format!(
                            "{}: can not relocate symbol {}",
                            self.name(),
                            str_name.unwrap()
                        )))?;
                        write_val(rela.r_offset as usize, symdef.tls.unwrap());
                    } else {
                        write_val(
                            rela.r_offset as usize,
                            self.extra_data().tls().unwrap() as usize,
                        );
                    }
                }
                #[cfg(feature = "tls")]
                REL_DTPOFF => {
                    let symdef = symdef.ok_or(relocate_error(format!(
                        "{}: can not relocate symbol {}",
                        self.name(),
                        str_name.unwrap()
                    )))?;
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
                        "{}:unsupport relocate type {}",
                        self.name(),
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
        if let Some(relro) = self.relro() {
            relro.relro()?;
        }
        self.set_user_data(func);
        self.set_dep_libs(libs);
        Ok(RelocatedLibrary {
            inner: Arc::new((AtomicBool::new(false), self.into())),
        })
    }
}
