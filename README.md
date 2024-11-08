[![](https://img.shields.io/crates/v/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![](https://img.shields.io/crates/d/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![license](https://img.shields.io/crates/l/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
# dlopen-rs

English | [中文](README-zh_cn.md)

A pure-rust library designed for loading ELF dynamic libraries from memory or from files. 

This library serves three purposes:
1. Provide a pure Rust alternative to musl/glibc ld.so.
2. Provide loading ELF dynamic libraries support for `#![no_std]` targets.
3. Easily swap out symbols in shared libraries with your own custom symbols at runtime

Additional, it integrates seamlessly with the system’s dynamic linker in `std` environments when the `ldso` feature is enabled. Currently, it supports `x86_64`, `x86`, `RV64`, and `AArch64` architectures.

## Feature
| Feature      | Default | Description                                                                                                                                           |
| ------------ | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| ldso         | Yes     | Allows dynamic libraries to be loaded using system dynamic loaders (ld.so).                                                                           |
| std          | Yes     | Enable `std`                                                                                                                                          |
| debug        | Yes     | Enable this to use gdb/lldb for debugging loaded dynamic libraries. Note that only dynamic libraries loaded using dlopen-rs can be debugged with gdb. |
| mmap_impl    | Yes     | Enable default implementation on platforms with mmap                                                                                                  |
| no_mmap_impl | No      | Enable default implementation on platforms without mmap                                                                                               |
| version      | No      | Activate specific versions of symbols for dynamic library loading                                                                                     |
| tls          | Yes     | Enable this to use thread local storage.                                                                                                              |
| nightly      | No      | Enable this for faster loading, but you’ll need to use the nightly compiler.                                                                          |
| unwinding    | No      | Enable this to use the exception handling mechanism provided by dlopen-rs.                                                                            |
| libgcc       | Yes     | Enable this if the program uses libgcc to handle exceptions.                                                                                          |
| libunwind    | No      | Enable this if the program uses libunwind to handle exceptions.                                                                                       |


## Examples

### Example 1
Fine-grained control over the dynamic library loading process, and the ability to replace certain functions in the dynamic library that need relocation with your own implementations. In this example, `malloc` is replaced with `mymalloc`.
```rust
use dlopen_rs::{ELFLibrary, MmapImpl};
use nix::libc::size_t;
use std::{ffi::c_void, path::Path};

extern "C" fn mymalloc(size: size_t) -> *mut c_void {
    println!("malloc:{}bytes", size);
    unsafe { nix::libc::malloc(size) }
}

fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();

    let libexample = ELFLibrary::from_file::<MmapImpl>(path)
        .unwrap()
        .relocate_with(&[libc, libgcc], |name| {
            if name == "malloc" {
                return Some(mymalloc as _);
            } else {
                return None;
            }
        })
        .unwrap();

    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");
}
```
### Example 2
Set the path for loading dynamic libraries:
```shell
export RUST_LD_LIBRARY_PATH=/lib32
```
Use the `dlopen` interface to load dynamic libraries:
```rust
use dlopen_rs::ELFLibrary;
use std::path::Path;

fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libexample = ELFLibrary::dlopen::<MmapImpl>(path).unwrap();
    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");
}
```
### Example 3
Users can implement their own   `Mmap`   trait so that   `dlopen-rs`   can work on various platforms.
The interfaces provided by   `dlopen-rs`   will have generics constrained by the   `Mmap`   trait, and users can use their own implementations. For example,   `MmapImpl`   in the following code is a default implementation provided by   `dlopen-rs`  , which users can replace with their own implementation.
```rust
let libexample = ELFLibrary::from_file::<MmapImpl>(path)
```
Below is an implementation of   `Mmap`   for systems with   `mmap`   provided by the   `dlopen-rs`   library. Note: This uses the   `nix`   crate.
```rust
struct MmapImpl

impl Mmap for MmapImpl {
    unsafe fn mmap(
        addr: Option<usize>,
        len: usize,
        prot: super::ProtFlags,
        flags: super::MapFlags,
        offset: super::Offset,
    ) -> crate::Result<core::ptr::NonNull<core::ffi::c_void>> {
        let addr = addr.map(|val| NonZeroUsize::new_unchecked(val));
        let length = NonZeroUsize::new_unchecked(len);
        let prot = mman::ProtFlags::from_bits(prot.bits()).unwrap();
        let flags = mman::MapFlags::from_bits(flags.bits()).unwrap();
        match offset.kind {
            super::OffsetType::File { fd, file_offset } => Ok(mman::mmap(
                addr,
                length,
                prot,
                flags,
                Fd(fd),
                // offset是当前段在文件中的偏移，需要按照页对齐，否则mmap会失败
                (file_offset & MASK) as _,
            )?),
            super::OffsetType::Addr(data_ptr) => {
                let ptr = mman::mmap_anonymous(addr, length, mman::ProtFlags::PROT_WRITE, flags)?;
                let dest =
                    from_raw_parts_mut(ptr.as_ptr().cast::<u8>().add(offset.offset), offset.len);
                let src = from_raw_parts(data_ptr, offset.len);
                dest.copy_from_slice(src);
                mman::mprotect(ptr, length.into(), prot)?;
                Ok(ptr)
            }
        }
    }

    unsafe fn mmap_anonymous(
        addr: usize,
        len: usize,
        prot: super::ProtFlags,
        flags: super::MapFlags,
    ) -> crate::Result<core::ptr::NonNull<core::ffi::c_void>> {
        Ok(mman::mmap_anonymous(
            Some(NonZeroUsize::new_unchecked(addr)),
            NonZeroUsize::new_unchecked(len),
            mman::ProtFlags::from_bits(prot.bits()).unwrap(),
            mman::MapFlags::from_bits(flags.bits()).unwrap(),
        )?)
    }

    unsafe fn munmap(addr: core::ptr::NonNull<core::ffi::c_void>, len: usize) -> crate::Result<()> {
        Ok(mman::munmap(addr, len)?)
    }

    unsafe fn mprotect(
        addr: core::ptr::NonNull<core::ffi::c_void>,
        len: usize,
        prot: super::ProtFlags,
    ) -> crate::Result<()> {
        mman::mprotect(addr, len, mman::ProtFlags::from_bits(prot.bits()).unwrap())?;
        Ok(())
    }
}
```
## NOTE
If you encounter any issues while using it or need any new features, feel free to raise an issue on GitHub. 