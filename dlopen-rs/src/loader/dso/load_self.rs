use std::{
    ffi::{c_int, c_void, CStr},
    mem::ManuallyDrop,
    ptr::NonNull,
    sync::Arc,
};

use elf::abi::PT_DYNAMIC;
use nix::libc::{dl_iterate_phdr, dl_phdr_info, size_t};

use crate::{
    find_lib_error,
    loader::{
        dso::{dynamic::ELFDynamic, hashtable::ELFHashTable, CommonElfData},
        RelocatedLibraryInner,
    },
    parse_dynamic_error, ELFLibrary, Error, RelocatedLibrary, Result,
};

use super::segment::ELFSegments;

impl ELFSegments {
    pub(crate) fn dummy(addr: usize) -> ELFSegments {
        ELFSegments {
            memory: unsafe { NonNull::new_unchecked(addr as *mut _) },
            offset: 0,
            len: isize::MAX as _,
        }
    }
}

impl ELFLibrary {
    /// Load the dynamic library used by the current program itself,
    /// you can load it using the name of the library
    /// # Examples
    ///
    /// ```no_run
    /// # use ::dlopen_rs::ELFLibrary;
    /// let libc = ELFLibrary::load_self("libc").unwrap();
    /// ```
    pub fn load_self(name: &str) -> Result<RelocatedLibrary> {
        unsafe extern "C" fn callback(
            info: *mut dl_phdr_info,
            _size: size_t,
            data: *mut c_void,
        ) -> c_int {
            let payload = &mut *data.cast::<PayLoad>();
            let name = payload.name;
            let info = &*info;
            let cur_name = CStr::from_ptr(info.dlpi_name).to_str().unwrap();
            if cur_name.contains(name) {
                let payload_data = &mut payload.data as *mut ManuallyDrop<CommonElfData>;
                let payload_err = &mut payload.err as *mut ManuallyDrop<Error>;
                let phdrs = core::slice::from_raw_parts(info.dlpi_phdr, info.dlpi_phnum as usize);
                let segments = ELFSegments::dummy(info.dlpi_addr as usize);

                let base = segments.base();
                let mut dynamics = None;
                for phdr in phdrs {
                    match phdr.p_type {
                        PT_DYNAMIC => {
                            dynamics = Some(
                                match ELFDynamic::new(core::mem::transmute(phdr), &segments) {
                                    Ok(dynamics) => dynamics,
                                    Err(err) => {
                                        payload_err.write(ManuallyDrop::new(err));
                                        return -1;
                                    }
                                },
                            )
                        }
                        _ => {}
                    }
                }

                let dynamics = if let Some(dynamics) = dynamics {
                    dynamics
                } else {
                    payload_err.write(ManuallyDrop::new(parse_dynamic_error(
                        "elf file does not have dynamic",
                    )));
                    return -1;
                };

                // musl和glibc有所区别，glibc会返回real addr。
                let symtab = dynamics.dynsym();
                let symtab = if symtab >= 2 * base {
                    symtab - base
                } else {
                    symtab
                };

                let strtab = dynamics.strtab();
                let strtab = if strtab.as_ptr() as usize >= 2 * base {
                    let ptr = strtab.as_ptr().sub(base);
                    core::slice::from_raw_parts(ptr, strtab.len())
                } else {
                    strtab
                };

                let hashtab = dynamics.hash();
                let hashtab = if hashtab >= 2 * base {
                    hashtab - base
                } else {
                    hashtab
                };

                let hashtab = ELFHashTable::parse_gnu_hash(hashtab as _);
                let strtab = elf::string_table::StringTable::new(strtab);
                let symtab = symtab as _;

                let common = ManuallyDrop::new(CommonElfData {
                    hashtab,
                    symtab,
                    strtab,
                    unwind: None,
                    segments,
                    fini_fn: None,
                    fini_array_fn: None,
                    #[cfg(feature = "tls")]
                    tls: None,
                });

                payload_data.write(common);

                return 1;
            }
            0
        }

        union PayLoad<'a> {
            name: &'a str,
            data: ManuallyDrop<CommonElfData>,
            err: ManuallyDrop<Error>,
        }

        let mut payload = PayLoad { name };

        let res = unsafe { dl_iterate_phdr(Some(callback), &mut payload as *mut PayLoad as _) };
        if res == 0 {
            return Err(find_lib_error(format!("can not open self lib: {}", name)));
        } else if res == -1 {
            return Err(unsafe { ManuallyDrop::into_inner(payload.err) });
        }

        let common = unsafe { ManuallyDrop::into_inner(payload.data) };

        let inner = RelocatedLibraryInner {
            common,
            internal_libs: Vec::new().into_boxed_slice(),
            external_libs: None,
        };
        Ok(RelocatedLibrary {
            inner: Arc::new(inner),
        })
    }
}
