use std::{
    ffi::{c_int, c_void, CStr},
    mem::ManuallyDrop,
    sync::Arc,
};

use elf::abi::PT_DYNAMIC;
use nix::libc::{dl_iterate_phdr, dl_phdr_info, size_t};

use crate::{
    dynamic::ELFDynamic,
    elfloader_error,
    hashtable::ELFHashTable,
    segment::ELFSegments,
    types::{CommonInner, RelocatedLibraryInner},
    ELFLibrary, RelocatedLibrary, Result,
};

impl ELFLibrary {
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
                let data = &mut payload.data as *mut ManuallyDrop<CommonInner>;
                let phdrs = core::slice::from_raw_parts(info.dlpi_phdr, info.dlpi_phnum as usize);
                let segments = ELFSegments::dump(info.dlpi_addr as usize);

                let base = segments.base();
                let mut dynamics = None;
                for phdr in phdrs {
                    match phdr.p_type {
                        PT_DYNAMIC => {
                            dynamics = Some(
                                ELFDynamic::new(core::mem::transmute(phdr), &segments).unwrap(),
                            )
                        }
                        _ => {}
                    }
                }

                let dynamics = dynamics.unwrap();

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

                let common = ManuallyDrop::new(CommonInner {
                    hashtab,
                    symtab,
                    strtab,
                    unwind: None,
                    segments,
                    fini_fn: None,
                    fini_array_fn: None,
                    tls: None,
                });

                data.write(common);

                return 1;
            }
            0
        }

        union PayLoad<'a> {
            name: &'a str,
            data: ManuallyDrop<CommonInner>,
        }

        let mut payload = PayLoad { name };

        let res = unsafe { dl_iterate_phdr(Some(callback), &mut payload as *mut PayLoad as _) };
        if res == 0 {
            return elfloader_error(format!("can not open self lib:{}", name));
        }

        let common = unsafe { ManuallyDrop::into_inner(payload.data) };

        let inner = RelocatedLibraryInner {
            common,
            needed_libs: Vec::new(),
            extern_lib: None,
        };
        Ok(RelocatedLibrary {
            inner: Arc::new(inner),
        })
    }
}
