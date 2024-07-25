# dlopen-rs

dlopen-rs supports loading dynamic libraries from memory and files, supports `no_std` environments, and does not rely on the dynamic linker `ldso`

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

    let f: dlopen_rs::Symbol<extern "C" fn(i32) -> i32> = unsafe { libexample.get("c_fun_add_two").unwrap() };
    println!("{}", f(2));

    let g: dlopen_rs::Symbol<extern "C" fn()> =
        unsafe { libexample.get("c_fun_print_something_else").unwrap() };
    g();

    let f: dlopen_rs::Symbol<extern "C" fn()> = unsafe { libexample.get("c_func_thread_local").unwrap() };
    f();
	
    let f: dlopen_rs::Symbol<extern "C" fn()> = unsafe { libexample.get("c_func_panic").unwrap() };
    f();
}
```

There is currently no support for using backtrace in loaded dynamic library code, and there is no support for debugging loaded dynamic libraries using gdb