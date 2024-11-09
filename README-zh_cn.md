[![](https://img.shields.io/crates/v/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![](https://img.shields.io/crates/d/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![license](https://img.shields.io/crates/l/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
# dlopen-rs
一个 `Rust` 库，用于从内存或文件加载 `ELF` 动态库。

这个库有三个目的：
1. 提供一个纯`Rust`编写的`musl/glibc` `ld.so`的替代方案。
2. 为 #![no_std] 目标提供加载 `ELF` 动态库的支持。
3. 能够轻松地在运行时用自己的自定义符号替换共享库中的符号。

## 特性

| 特性         | 是否默认开启 | 描述                                                                                               |
| ------------ | ------------ | -------------------------------------------------------------------------------------------------- |
| ldso         | 是           | 允许使用系统动态加载器（ld.so）加载动态库。                                                        |
| std          | 是           | 启用Rust标准库                                                                                     |
| debug        | 否           | 启用后可以使用 gdb/lldb 调试已加载的动态库。注意，只有使用 dlopen-rs 加载的动态库才能用 gdb 调试。 |
| mmap_impl    | 是           | 启用在有mmap的平台上的默认实现                                                                     |
| no_mmap_impl | 否           | 启用在没有mmap的平台上的默认实现                                                                   |
| version      | 否           | 允许使用符号的版本号                                                                               |
| tls          | 是           | 启用后可以使用线程本地存储。                                                                       |
| nightly      | 否           | 启用后可以更快地加载，但需要使用nightly编译器。                                                    |
| unwinding    | 否           | 启用后可以使用 dlopen-rs 提供的异常处理机制。                                                      |
| libgcc       | 是           | 如果程序使用 libgcc 处理异常，启用此特性。                                                         |
| libunwind    | 否           | 如果程序使用 libunwind 处理异常，启用此特性。                                                      |
## 示例

### 示例1
细粒度地控制动态库的加载流程,并且可以将动态库中需要重定位的某些函数换成自己实现的函数。下面这个例子中就是把`malloc`替换为了`mymalloc`。
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
### 示例2
设置加载动态库的路径:
```shell
export RUST_LD_LIBRARY_PATH=/lib32
```
使用`dlopen`接口加载动态库:
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
### 示例3
用户可以自己实现`Mmap` trait，使`dlopen-rs`可以工作在各种平台上。  
`dlopen-rs`提供的接口会带有具有`Mmap` trait约束的泛型，用户可以使用自己的实现。例如下面代码中的`MmapImpl`就是`dlopen-rs`提供的一种默认实现，用户可以将其替换为自己的实现。
```rust
let libexample = ELFLibrary::from_file::<MmapImpl>(path)
```
下面是`dlopen-rs`库自带的一种在操作系统有`mmap`情况下的实现，实现时用到了`nix`库。
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

## 补充

如果您在使用过程中遇到问题可以在 GitHub 上提出问题。如果`dlopen-rs`对您有帮助的话，不妨点个`star`。^V^
