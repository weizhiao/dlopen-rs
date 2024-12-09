[![](https://img.shields.io/crates/v/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![](https://img.shields.io/crates/d/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![license](https://img.shields.io/crates/l/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
# dlopen-rs
一个 `Rust` 库，用于从内存或文件加载 `ELF` 动态库。

这个库有四个目的：
1. 提供一个纯`Rust`编写的动态链接器。
2. 为 #![no_std] 目标提供加载 `ELF` 动态库的支持。
3. 能够轻松地在运行时用自己的自定义符号替换共享库中的符号。
4. 大多数情况下有比`ld.so`更快的速度（加载动态库和获取符号）

## 特性

| 特性      | 是否默认开启 | 描述                                                                                               |
| --------- | ------------ | -------------------------------------------------------------------------------------------------- |
| ldso      | 是           | 允许使用系统动态加载器（ld.so）加载动态库。                                                        |
| std       | 是           | 启用Rust标准库                                                                                     |
| debug     | 否           | 启用后可以使用 gdb/lldb 调试已加载的动态库。注意，只有使用 dlopen-rs 加载的动态库才能用 gdb 调试。 |
| mmap      | 是           | 启用在有mmap的平台上的默认实现                                                                     |  |
| version   | 否           | 在寻找符号时使用符号的版本号                                                                       |
| tls       | 是           | 启用后动态库中可以使用线程本地存储。                                                               |  |
| unwinding | 否           | 启用后可以使用 dlopen-rs 提供的异常处理机制。                                                      |
| libgcc    | 是           | 如果程序使用 libgcc 处理异常，启用此特性。                                                         |
| libunwind | 否           | 如果程序使用 libunwind 处理异常，启用此特性。                                                      |
## 示例

### 示例1
细粒度地控制动态库的加载流程,可以将动态库中需要重定位的某些函数换成自己实现的函数,并且可以手动控制是否开启符号的延迟绑定(lazy binding)。  
下面这个例子中就是把动态库中的`malloc`替换为了`mymalloc`，且开启了延迟绑定。
```rust
use dlopen_rs::ELFLibrary;
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
### 示例2
设置加载动态库的路径和是否启用延迟绑定。
* 当LD_BIND_NOW = 0时强制开启延迟绑定。
* 当LD_BIND_NOW = 1时强制关闭延迟绑定。
* 当LD_BIND_NOW未设置时由动态库自身的编译参数决定。
```shell
export LD_LIBRARY_PATH=/lib
export LD_BIND_NOW = 0
```
使用`dlopen`接口加载动态库:
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
## 补充
如果您在使用过程中遇到问题可以在 GitHub 上提出问题。如果`dlopen-rs`对您有帮助的话，不妨点个`star`。^V^
