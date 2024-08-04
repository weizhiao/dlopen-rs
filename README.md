[![](https://img.shields.io/crates/v/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![](https://img.shields.io/crates/d/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
[![license](https://img.shields.io/crates/l/dlopen-rs.svg)](https://crates.io/crates/dlopen-rs)
# dlopen-rs

dlopen-rs supports loading dynamic libraries from memory and files, supports `no_std` environments, and does not rely on the dynamic linker `ldso`

Currently supports `x86_64`, `x86`, `RV64` and `AArch64`.

## Feature
| Feature              | Default | Description |
|--------------------- |---------|-|
| load_self            | Yes     | Enable load the dso of the program itself |
| std          | Yes     | Enable `std` |
| mmap         | Yes      | Enable this on platforms that support `mmap` |
| tls         | Yes     | Enable this when you need to use `thread local storage` |
| nightly | No      | Enable this can make loading `faster`, but you'll need to use the `nightly` compiler |
| unwinding           | No      | Enable this when you want to use the exception handling mechanism provided by dlopen-rs  |
| libgcc            | Yes      | Enable this when program uses `libgcc` to handle exceptions |
| libunwind          | No     | Enable this when program uses `libunwind` to handle exceptions |

## Example
```rust
use dlopen_rs::ELFLibrary;
use std::path::Path;

fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libc = ELFLibrary::load_self("libc").unwrap();
    let libgcc = ELFLibrary::load_self("libgcc").unwrap();
    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate(&[libgcc, libc])
        .unwrap();
    let f = unsafe {
        libexample
            .get::<extern "C" fn(i32) -> i32>("c_fun_add_two")
            .unwrap()
    };
    println!("{}", f(2));

    let f = unsafe {
        libexample
            .get::<extern "C" fn()>("c_fun_print_something_else")
            .unwrap()
    };
    f();
}
```
## NOTE
There is currently no support for using backtrace in loaded dynamic library code, and there is no support for debugging loaded dynamic libraries using gdb