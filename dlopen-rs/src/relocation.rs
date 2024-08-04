use crate::segment::ELFRelro;
use crate::types::ExternLibrary;
use crate::{arch::*, relocate_error};
use crate::{
    builtin::BUILTIN,
    types::{ELFLibrary, RelocatedLibrary},
    Rela, Result, REL_BIT, REL_MASK,
};
use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use elf::abi::*;

#[derive(Debug)]
pub(crate) struct ELFRelocation {
    pub(crate) pltrel: Option<&'static [Rela]>,
    pub(crate) rel: Option<&'static [Rela]>,
    pub(crate) relro: Option<ELFRelro>,
}

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
        self.relocate_with(internal_libs, None)
    }

    /// use internal and external dependency libraries to relocate the current library
    pub fn relocate_with(
        self,
        internal_libs: &[RelocatedLibrary],
        external_libs: Option<Vec<Box<dyn ExternLibrary + 'static>>>,
    ) -> Result<RelocatedLibrary> {
        let pltrel = if let Some(pltrel) = self.relocation().pltrel {
            pltrel.iter()
        } else {
            [].iter()
        };

        let rela = if let Some(rela) = self.relocation().rel {
            rela.iter()
        } else {
            [].iter()
        };

        /*
            A Represents the addend used to compute the value of the relocatable field.
            B Represents the base address at which a shared object has been loaded into memory during execution.
            S Represents the value of the symbol whose index resides in the relocation entry.
        */

        for rela in rela.chain(pltrel) {
            let r_type = rela.r_info as usize & REL_MASK;
            match r_type as _ {
                // do nothing
                REL_NONE => {}
                // REL_GOT/REL_JUMP_SLOT: S  REL_SYMBOLIC: S + A
                REL_JUMP_SLOT | REL_GOT | REL_SYMBOLIC => {
                    let r_sym = rela.r_info as usize >> REL_BIT;
                    let dynsym = unsafe { self.symtab().add(r_sym).read() };
                    let append = rela.r_addend;
                    let symbol = if dynsym.st_info >> 4 == STB_LOCAL {
                        dynsym.st_value as _
                    } else {
                        let name = self
                            .strtab()
                            .get(dynsym.st_name as usize)
                            .map_err(relocate_error)?;

                        let symbol = BUILTIN
                            .get(&name)
                            .copied()
                            .or_else(|| {
                                if dynsym.st_shndx != SHN_UNDEF {
                                    return Some(unsafe {
                                        self.segments()
                                            .as_mut_ptr()
                                            .add(dynsym.st_value as usize)
                                            .cast()
                                    });
                                }

                                for lib in internal_libs.iter() {
                                    if let Some(sym) = lib.get_sym(name) {
                                        return Some(sym);
                                    }
                                }

                                if let Some(libs) = external_libs.as_ref() {
                                    for lib in libs.iter() {
                                        if let Some(sym) = lib.get_sym(name) {
                                            return Some(sym);
                                        }
                                    }
                                }
                                None
                            })
                            .ok_or_else(|| {
                                relocate_error(format!("can not relocate symbol {}", name))
                            })?;
                        symbol
                    };

                    let rel_addr = unsafe {
                        self.segments()
                            .as_mut_ptr()
                            .add(rela.r_offset.checked_add_signed(append).unwrap() as usize)
                            as *mut usize
                    };

                    unsafe { rel_addr.write(symbol as usize) }
                }
                // B + A
                REL_RELATIVE => {
                    let rel_addr = unsafe {
                        self.segments().as_mut_ptr().add(rela.r_offset as usize) as *mut usize
                    };
                    unsafe { rel_addr.write(self.segments().base() + rela.r_addend as usize) }
                }
                #[cfg(feature = "tls")]
                REL_DTPMOD => {
                    let rel_addr = unsafe {
                        self.segments().as_mut_ptr().add(rela.r_offset as usize) as *mut usize
                    };
                    unsafe { rel_addr.write(self.tls() as usize) }
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

        if let Some(relro) = &self.relocation().relro {
            relro.relro()?;
        }

        Ok(RelocatedLibrary::new(
            self,
            internal_libs.to_vec(),
            external_libs,
        ))
    }
}
