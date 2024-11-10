use crate::builtin::BUILTIN;
use crate::loader::*;
use crate::relocate_error;
use crate::ELFLibrary;
use crate::RelocatedLibrary;
use crate::Result;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::AtomicBool;
use elf::abi::*;

#[allow(unused)]
struct SymDef<'temp> {
    sym: &'temp ELFSymbol,
    base: usize,
    #[cfg(feature = "tls")]
    tls: Option<usize>,
}

impl From<SymDef<'_>> for *const () {
    fn from(symdef: SymDef<'_>) -> Self {
        if symdef.sym.st_info & 0xf != STT_GNU_IFUNC {
            (symdef.base + symdef.sym.st_value as usize) as _
        } else {
            let ifunc: fn() -> usize =
                unsafe { core::mem::transmute(symdef.base + symdef.sym.st_value as usize) };
            ifunc() as _
        }
    }
}

pub(crate) struct UserData {
    data: Vec<Box<dyn Fn(&str) -> Option<*const ()> + 'static>>,
}

impl UserData {
    pub(crate) const fn empty() -> Self {
        Self { data: Vec::new() }
    }
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
    pub fn relocate(self, libs: impl AsRef<[RelocatedLibrary]>) -> Self {
        let find_symbol = move |name: &str| -> Option<*const ()> { BUILTIN.get(name).copied() };
        self.relocate_impl(libs.as_ref(), Box::new(find_symbol))
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
    ///		.unwrap();
    /// ```
    /// # Note
    /// It will use function closure to relocate current lib firstly
    pub fn relocate_with<F>(self, libs: impl AsRef<[RelocatedLibrary]>, func: F) -> Self
    where
        F: Fn(&str) -> Option<*const ()> + 'static,
    {
        let find_symbol =
            move |name: &str| -> Option<*const ()> { BUILTIN.get(name).copied().or(func(name)) };
        self.relocate_impl(libs.as_ref(), Box::new(find_symbol))
    }

    fn relocate_impl(
        mut self,
        libs: &[RelocatedLibrary],
        find_symbol: Box<dyn Fn(&str) -> Option<*const ()> + 'static>,
    ) -> Self {
        let mut relocation = self.relocation();
        let find_symdef =
            |dynsym: &'static ELFSymbol, syminfo: SymbolInfo<'_>| -> Option<SymDef<'_>> {
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
            };

        /*
            A Represents the addend used to compute the value of the relocatable field.
            B Represents the base address at which a shared object has been loaded into memory during execution.
            S Represents the value of the symbol whose index resides in the relocation entry.
        */

        if let Some(rela_array) = &mut relocation.pltrel {
            rela_array.relocate(|rela, idx, bitmap, deal_fail| {
                let r_type = rela.r_info as usize & REL_MASK;
                let r_sym = rela.r_info as usize >> REL_BIT;
                assert!(r_sym != 0);
                let (dynsym, syminfo) = self.symbols().rel_symbol(r_sym);
                let symbol = if let Some(symbol) = find_symbol(syminfo.name)
                    .or(find_symdef(dynsym, syminfo).map(|symdef| symdef.into()))
                {
                    symbol
                } else {
                    deal_fail(idx, bitmap);
                    return;
                };
                match r_type as _ {
                    // S
                    // 对于.rela.plt来说只有这一种重定位类型
                    REL_JUMP_SLOT => {
                        self.write_val(rela.r_offset as usize, symbol as usize);
                    }
                    _ => {
                        unreachable!()
                    }
                }
            });
        }

        if let Some(rela_array) = &mut relocation.dynrel {
            rela_array.relocate(|rela, idx, bitmap, deal_fail| {
                let r_type = rela.r_info as usize & REL_MASK;
                let r_sym = rela.r_info as usize >> REL_BIT;
                let mut name = "";
                let symdef = if r_sym != 0 {
                    let (dynsym, syminfo) = self.symbols().rel_symbol(r_sym);
                    name = syminfo.name;
                    find_symdef(dynsym, syminfo)
                } else {
                    None
                };

                match r_type as _ {
                    // do nothing
                    REL_NONE => {}
                    // REL_GOT: S  REL_SYMBOLIC: S + A
                    REL_GOT | REL_SYMBOLIC => {
                        let symbol = if let Some(symbol) =
                            find_symbol(name).or(symdef.map(|symdef| symdef.into()))
                        {
                            symbol
                        } else {
                            deal_fail(idx, bitmap);
                            return;
                        };
                        self.write_val(
                            rela.r_offset as usize,
                            symbol as usize + rela.r_addend as usize,
                        );
                    }
                    // B + A
                    REL_RELATIVE => {
                        self.write_val(
                            rela.r_offset as usize,
                            self.extra_data().base() + rela.r_addend as usize,
                        );
                    }
                    // ELFTLS
                    #[cfg(feature = "tls")]
                    REL_DTPMOD => {
                        if r_sym != 0 {
                            let symdef = if let Some(symdef) = symdef {
                                symdef
                            } else {
                                deal_fail(idx, bitmap);
                                return;
                            };
                            self.write_val(rela.r_offset as usize, symdef.tls.unwrap());
                        } else {
                            self.write_val(
                                rela.r_offset as usize,
                                self.extra_data().tls().unwrap() as usize,
                            );
                        }
                    }
                    #[cfg(feature = "tls")]
                    REL_DTPOFF => {
                        let symdef = if let Some(symdef) = symdef {
                            symdef
                        } else {
                            deal_fail(idx, bitmap);
                            return;
                        };
                        // offset in tls
                        let tls_val = (symdef.sym.st_value as usize + rela.r_addend as usize)
                            .wrapping_sub(TLS_DTV_OFFSET);
                        self.write_val(rela.r_offset as usize, tls_val);
                    }
                    _ => {
                        // REL_TPOFF：这种类型的重定位明显做不到，它是为静态模型设计的，这种方式
                        // 可以通过带偏移量的内存读取来获取TLS变量，无需使用__tls_get_addr，
                        // 实现它需要对要libc做修改，因为它要使用tp来访问thread local，
                        // 而线程栈里保存的东西完全是由libc控制的
                    }
                }
            });
        }

        self.user_data().data.push(find_symbol);
        self.insert_dep_libs(libs);
        self.set_relocation(relocation);
        self
    }

    #[inline(always)]
    fn write_val(&self, offset: usize, val: usize) {
        unsafe {
            let rel_addr = (self.extra_data().base() + offset) as *mut usize;
            rel_addr.write(val)
        };
    }

    pub fn finish(mut self) -> Result<RelocatedLibrary> {
        if !self.is_finished() {
            return Err(relocate_error(self.relocation().not_relocated(&self)));
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
        Ok(RelocatedLibrary {
            inner: Arc::new((AtomicBool::new(false), self.into())),
        })
    }
}
