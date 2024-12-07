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

Additional, it integrates seamlessly with the system’s dynamic linker in `std` environments when the `ldso` feature is enabled. Currently, it supports `x86_64`, `RV64`, and `AArch64` architectures.

## Feature
| Feature   | Default | Description                                                                                                                                           |
| --------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| ldso      | Yes     | Allows dynamic libraries to be loaded using system dynamic loaders (ld.so).                                                                           |
| std       | Yes     | Enable `std`                                                                                                                                          |
| debug     | No      | Enable this to use gdb/lldb for debugging loaded dynamic libraries. Note that only dynamic libraries loaded using dlopen-rs can be debugged with gdb. |
| mmap      | Yes     | Enable default implementation on platforms with mmap                                                                                                  |  |
| version   | No      | Activate specific versions of symbols for dynamic library loading                                                                                     |
| tls       | Yes     | Enable this to use thread local storage.                                                                                                              |  |
| unwinding | No      | Enable this to use the exception handling mechanism provided by dlopen-rs.                                                                            |
| libgcc    | Yes     | Enable this if the program uses libgcc to handle exceptions.                                                                                          |
| libunwind | No      | Enable this if the program uses libunwind to handle exceptions.                                                                                       |


## Examples

### Example 1
Fine-grained control over the dynamic library loading process, and the ability to replace certain functions in the dynamic library that need relocation with your own implementations. In this example, `malloc` is replaced with `mymalloc`.
```rust
use dlopen_rs::{ELFLibrary};
use libc::size_t;
use std::{ffi::c_void, path::Path};

extern "C" fn mymalloc(size: size_t) -> *mut c_void {
    println!("malloc:{}bytes", size);
    unsafe { libc::malloc(size) }
}

fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();

    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate_with(&[libc, libgcc], |name| {
            if name == "malloc" {
                return Some(mymalloc as _);
            } else {
                return None;
            }
        })
        .finish()
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
    let libexample = ELFLibrary::dlopen(path).unwrap();
    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");
}
```
## NOTE
If you encounter any issues while using the library, feel free to raise an issue on GitHub. If   `dlopen-rs`   has been helpful to you, don't hesitate to give it a   `star`  . ^V^