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
| Feature   | Default | Description                                                                                                                                           |
| --------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| ldso      | Yes     | Allows dynamic libraries to be loaded using system dynamic loaders (ld.so).                                                                           |
| std       | Yes     | Enable `std`                                                                                                                                          |
| debug     | Yes     | Enable this to use gdb/lldb for debugging loaded dynamic libraries. Note that only dynamic libraries loaded using dlopen-rs can be debugged with gdb. |
| mmap      | Yes     | Enable this on platforms that support `mmap`                                                                                                          |
| version   | No      | Activate specific versions of symbols for dynamic library loading                                                                                     |
| tls       | Yes     | Enable this to use thread local storage.                                                                                                              |
| nightly   | No      | Enable this for faster loading, but you’ll need to use the nightly compiler.                                                                          |
| unwinding | No      | Enable this to use the exception handling mechanism provided by dlopen-rs.                                                                            |
| libgcc    | Yes     | Enable this if the program uses libgcc to handle exceptions.                                                                                          |
| libunwind | No      | Enable this if the program uses libunwind to handle exceptions.                                                                                       |


## Examples

### Example 1
Fine-grained control over the dynamic library loading process:
```rust
use dlopen_rs::ELFLibrary;
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
    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate_with_func(&[libc, libgcc], |name| {
            if name == "malloc" {
                return Some(mymalloc as _);
            } else {
                return None;
            }
        })
        .unwrap();
    let add = unsafe {
        libexample
            .get::<fn(i32, i32) -> i32>("add")
            .unwrap()
    };
    println!("{}", f(1,1));

    let print = unsafe {
        libexample
            .get::<fn(&str)>("print")
            .unwrap()
    };
    f("dlopen-rs: hello world!");
}
```
### Example 2
Set the path to load the dynamic library:
```shell
export RUST_LD_LIBRARY_PATH=/lib32
```
Load dynamic libraries using the dlopen interface:
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
If you encounter any issues while using it or need any new features, feel free to raise an issue on GitHub. 