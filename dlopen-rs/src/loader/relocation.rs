use super::{arch::*, builtin::BUILTIN, dso::ELFLibrary, ExternLibrary};
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

        struct SymDef<'temp> {
            /// symbol name
            name: &'temp str,
            /// None->fail/external lib's sym  Some(..)->self/internal lib
            sym: Option<&'temp ELFSymbol>,
            /// None->self/external lib  Some(..)->internal lib
            dso: Option<&'temp RelocatedLibrary>,
        }

        let write_val = |addr: u64, symbol: usize| {
            unsafe {
                let rel_addr = (self.base() + addr as usize) as *mut usize;
                rel_addr.write(symbol)
            };
        };

        for rela in iter {
            let r_type = rela.r_info as usize & REL_MASK;
            let r_sym = rela.r_info as usize >> REL_BIT;
            let symdef = if r_sym != 0 {
                let dynsym = unsafe { &*self.symtab().add(r_sym) };
                let name = self
                    .strtab()
                    .get(dynsym.st_name as usize)
                    .map_err(relocate_error)?;

                let symdef = if dynsym.st_shndx != SHN_UNDEF {
                    SymDef {
                        name,
                        sym: Some(dynsym),
                        dso: None,
                    }
                } else {
                    let mut symbol = SymDef {
                        name,
                        sym: None,
                        dso: None,
                    };
                    for lib in internal_libs.iter() {
                        if let Some(sym) = lib.get_sym(name) {
                            symbol.sym = Some(sym);
                            symbol.dso = Some(lib);
                            break;
                        }
                    }
                    symbol
                };
                Some(symdef)
            } else {
                None
            };

            // search order: BUILTIN -> internal_libs -> external_libs
            let find_symbol = || -> Result<*const ()> {
                let symdef = symdef.as_ref().unwrap();
                let name = symdef.name;
                BUILTIN
                    .get(name)
                    .copied()
                    .or_else(|| {
                        symdef.sym.map(|sym| {
                            symdef
                                .dso
                                .map(|dso| (dso.base() + sym.st_value as usize) as _)
                                .unwrap_or((self.base() + sym.st_value as usize) as _)
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
                    write_val(rela.r_offset, symbol as usize + rela.r_addend as usize);
                }
                // B + A
                REL_RELATIVE => {
                    write_val(rela.r_offset, self.base() + rela.r_addend as usize);
                }
                // ELFTLS
                #[cfg(feature = "tls")]
                REL_DTPMOD => {
                    if let Some(ref symdef) = symdef {
                        if symdef.sym.is_none() {
                            return Err(relocate_error(format!(
                                "can not relocate symbol {}",
                                symdef.name
                            )));
                        }
                    }
                    // write the dependent dso's tls
                    write_val(
                        rela.r_offset,
                        symdef
                            .map(|symdef| {
                                symdef.dso.map(|dso| dso.get_tls()).unwrap_or(self.tls()) as usize
                            })
                            .unwrap_or(self.tls() as usize),
                    );
                }
                #[cfg(feature = "tls")]
                REL_DTPOFF => {
                    let symdef = symdef.unwrap();
                    let symbol = symdef.sym.ok_or(relocate_error(format!(
                        "can not relocate symbol {}",
                        symdef.name
                    )))?;
                    write_val(
                        rela.r_offset,
                        (symbol.st_value as usize + rela.r_addend as usize)
                            .wrapping_sub(TLS_DTV_OFFSET),
                    );
                }
                #[cfg(feature = "tls")]
                REL_TLSDESC => {
                    todo!()
                    // use crate::arch::TLSIndex;
                    // let tls_index = Box::new(TLSIndex {
                    //     ti_module: self.tls() as usize,
                    //     ti_offset: rela.r_addend as usize,
                    // });
                    // let rel_addr = unsafe {
                    //     self.segments().as_mut_ptr().add(rela.r_offset as usize) as *mut usize
                    // };
                    // unsafe {
                    //     rel_addr.write(crate::tls::tls_get_addr as usize);
                    //     rel_addr
                    //         .add(1)
                    //         .write(Box::leak(tls_index) as *const TLSIndex as usize)
                    // };
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
