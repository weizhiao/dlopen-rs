[![](https://img.shields.io/crates/v/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![](https://img.shields.io/crates/d/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![license](https://img.shields.io/crates/l/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
# dlopen-rs
一个 Rust 库，用于从内存或文件加载 ELF 动态库。

这个库有三个目的：
1. 提供一个纯`Rust`编写的musl/glibc ld.so的替代方案。
2. 为 #![no_std] 目标提供加载 `ELF` 动态库的支持。
3. 能够轻松地在运行时用自己的自定义符号替换共享库中的符号。

## 特性

| 特性      | 是否默认开启 | 描述                                                                                               |
| --------- | ------------ | -------------------------------------------------------------------------------------------------- |
| ldso      | 是           | 允许使用系统动态加载器（ld.so）加载动态库。                                                        |
| std       | 是           | 启用Rust标准库                                                                                     |
| debug     | 是           | 启用后可以使用 gdb/lldb 调试已加载的动态库。注意，只有使用 dlopen-rs 加载的动态库才能用 gdb 调试。 |
| mmap      | 是           | 在支持 mmap 的平台上启用                                                                           |
| version   | 否           | 允许使用符号的版本号                                                                               |
| tls       | 是           | 启用后可以使用线程本地存储。                                                                       |
| nightly   | 否           | 启用后可以更快地加载，但需要使用nightly编译器。                                                    |
| unwinding | 否           | 启用后可以使用 dlopen-rs 提供的异常处理机制。                                                      |
| libgcc    | 是           | 如果程序使用 libgcc 处理异常，启用此特性。                                                         |
| libunwind | 否           | 如果程序使用 libunwind 处理异常，启用此特性。                                                      |
## 示例

### 示例1
细粒度地控制动态库的加载流程
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
### 示例2
设置加载动态库的路径:
```shell
export RUST_LD_LIBRARY_PATH=/lib32
```
使用dlopen接口加载动态库:
```Rust
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

## 补充

如果您在使用过程中遇到任何问题或需要任何新特性，请随时在 GitHub 上提出问题。
