use super::{arch::*, builtin::BUILTIN, dso::ELFLibrary, ExternLibrary};
use crate::loader::dso::CommonElfData;
use crate::relocate_error;
use crate::RelocatedLibrary;
use crate::Result;
use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use elf::abi::*;

impl ELFLibrary {
    /// use internal dependent libraries to relocate the current library
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
    pub fn relocate(self, internal_libs: &[RelocatedLibrary]) -> Result<RelocatedLibrary> {
        self.relocate_impl(internal_libs, None)
    }

    /// use internal and external dependency libraries to relocate the current library
    pub fn relocate_with<T>(
        self,
        internal_libs: &[RelocatedLibrary],
        external_libs: Vec<T>,
    ) -> Result<RelocatedLibrary>
    where
        T: ExternLibrary + 'static,
    {
        let external_libs: Vec<Box<dyn ExternLibrary + 'static>> = external_libs
            .into_iter()
            .map(|lib| Box::new(lib) as Box<dyn ExternLibrary + 'static>)
            .collect();
        self.relocate_impl(internal_libs, Some(external_libs))
    }

    fn relocate_impl(
        self,
        internal_libs: &[RelocatedLibrary],
        external_libs: Option<Vec<Box<dyn ExternLibrary + 'static>>>,
    ) -> Result<RelocatedLibrary> {
        let iter = self
            .relocation()
            .pltrel
            .unwrap_or(&[])
            .iter()
            .chain(self.relocation().rel.unwrap_or(&[]));

        struct InternalSym<'temp> {
            sym: &'temp ELFSymbol,
            dso: &'temp CommonElfData,
        }

		#[allow(unused)]
        enum SymDef<'temp> {
            Internal(InternalSym<'temp>),
            External(*const ()),
        }

        let write_val = |addr: usize, symbol: usize| {
            unsafe {
                let rel_addr = (self.common_data().base() + addr) as *mut usize;
                rel_addr.write(symbol)
            };
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
                    for lib in internal_libs.iter() {
                        match lib.inner.as_ref() {
                            crate::loader::RelocatedLibraryInner::Internal(lib) => {
                                if let Some(sym) = lib.get_sym(temp_str_name) {
                                    symbol = Some(SymDef::Internal(InternalSym {
                                        sym,
                                        dso: lib.common_data(),
                                    }));
                                    break;
                                }
                            }
							#[cfg(feature = "ldso")]
                            crate::loader::RelocatedLibraryInner::External(lib) => {
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

            // search order: BUILTIN -> internal_libs -> external_libs
            let find_symbol = || -> Result<*const ()> {
                let name = str_name.unwrap();
                BUILTIN
                    .get(name)
                    .copied()
                    .or_else(|| {
                        symdef.as_ref().map(|symdef| match symdef {
                            SymDef::Internal(sym) => {
                                (sym.dso.base() + sym.sym.st_value as usize) as _
                            }
                            SymDef::External(sym) => *sym,
                        })
                    })
                    .or_else(|| {
                        if let Some(libs) = external_libs.as_ref() {
                            for lib in libs.iter() {
                                if let Some(sym) = lib.get_sym(name) {
                                    return Some(sym);
                                }
                            }
                        }
                        None
                    })
                    .ok_or(relocate_error(format!("can not relocate symbol {}", name)))
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
                    let symbol = find_symbol()?;
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

        Ok(RelocatedLibrary::new(
            self,
            internal_libs.to_vec(),
            external_libs,
        ))
    }
}
