[![](https://img.shields.io/crates/v/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![](https://img.shields.io/crates/d/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![license](https://img.shields.io/crates/l/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
# dlopen-rs

English | [中文](README-zh_cn.md)

A pure-rust library designed for loading ELF dynamic libraries from memory or from files. 

This library serves four purposes:
1. Provides a dynamic linker written purely in `Rust`.
2. Provide loading ELF dynamic libraries support for `#![no_std]` targets.
3. Easily swap out symbols in shared libraries with your own custom symbols at runtime
4. Faster than `ld.so` in most cases (loading dynamic libraries and getting symbols)

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
Fine-grained control of the load flow of the dynamic library, you can replace certain functions in the dynamic library that need to be relocated with your own implementation, and you can manually control whether the lazy binding of symbols is turned on.  
In the following example, we replace `malloc` with `mymalloc` in the dynamic library and turn on lazy binding.
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

    let libexample = ELFLibrary::from_file(path, Some(true))
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
Sets the path to load dynamic libraries and whether to enable lazy binding.  
* Lazy binding is forcibly turned on when LD_BIND_NOW = 0.
* Lazy binding is forcibly turned off when LD_BIND_NOW = 1.
* Lazy binding is determined by the compilation parameter of the dynamic library itself when LD_BIND_NOW is not set.
```shell
export LD_LIBRARY_PATH=/lib
export LD_BIND_NOW = 0
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