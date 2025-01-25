[![](https://img.shields.io/crates/v/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![](https://img.shields.io/crates/d/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![license](https://img.shields.io/crates/l/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
# dlopen-rs

English | [中文](README-zh_cn.md)  

[[Documentation]](https://docs.rs/dlopen-rs/)

A Rust library that implements a series of interfaces such as `dlopen` and `dlsym`, consistent with the behavior of libc, providing robust support for dynamic library loading and symbol resolution.

This library serves four purposes:
1. Provides a dynamic linker written purely in `Rust`.
2. Provide loading ELF dynamic libraries support for `#![no_std]` targets.
3. Easily swap out symbols in shared libraries with your own custom symbols at runtime
4. Faster than `ld.so` in most cases (loading dynamic libraries and getting symbols)

Currently, it supports `x86_64`, `RV64`, and `AArch64` architectures.

## Feature
| Feature   | Default | Description                                                                                                                                           |
| --------- | ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |                                                  
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
The `dlopen` interface is used to load dynamic libraries. The `dlopen` in `dlopen-rs` behaves consistently with the `dlopen` in `libc`. Additionally, this library uses the `log` crate, and you can use your preferred logging library to output logs to observe the working process of `dlopen-rs`. In the examples of this library, the `env_logger` crate is used for logging.
```rust
use dlopen_rs::ELFLibrary;
use std::path::Path;

fn main() {
    std::env::set_var("RUST_LOG", "trace");
    env_logger::init();
    dlopen_rs::init();
    let path = Path::new("./target/release/libexample.so");
    let libexample =
        ElfLibrary::dlopen(path, OpenFlags::RTLD_LOCAL | OpenFlags::RTLD_LAZY).unwrap();
    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");
}
```

### Example 2
Use `LD_PRELOAD` to replace the dlopen and other functions in libc with the implementations provided by this library.
```shell
# Compile the library into a dynamic library format
cargo build -r -p cdylib

# Compile the test case
cargo build -r -p dlopen-rs --example preload

# Use the implementation from this library to replace the implementation in libc
RUST_LOG=trace LD_PRELOAD=./target/release/libdlopen.so ./target/release/examples/preload
```

### Example 3
Fine-grained control of the load flow of the dynamic library, you can replace certain functions in the dynamic library that need to be relocated with your own implementation. In the following example, we replace `malloc` with `mymalloc` in the dynamic library and turn on lazy binding.
```rust
use dlopen_rs::ELFLibrary;
use libc::size_t;
use std::{ffi::c_void, path::Path};

extern "C" fn mymalloc(size: size_t) -> *mut c_void {
    println!("malloc:{}bytes", size);
    unsafe { libc::malloc(size) }
}

fn main() {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    dlopen_rs::init();
    let path = Path::new("./target/release/libexample.so");
    let libc = ElfLibrary::load_existing("libc.so.6").unwrap();
    let libgcc = ElfLibrary::load_existing("libgcc_s.so.1").unwrap();

    let libexample = ElfLibrary::from_file(path, OpenFlags::CUSTOM_NOT_REGISTER)
        .unwrap()
        .relocate_with(&[libc, libgcc], &|name: &str| {
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

## TODO
* dladdr and dlinfo have not been implemented yet. dlerror currently only returns NULL.  
* RTLD_NEXT for dlsym has not been implemented.
* The library currently cannot search for dependent dynamic libraries in ld.so.cache.
* When dlopen fails, the newly loaded dynamic library is destroyed, but the functions in .fini are not called.
* It is unclear whether there is a way to support more relocation types.
* There is a lack of correctness and performance testing under high-concurrency multithreading scenarios.

## Supplement
If you encounter any issues during use, feel free to raise them on GitHub. We warmly welcome everyone to contribute code to help improve the functionality of dlopen-rs. 😊