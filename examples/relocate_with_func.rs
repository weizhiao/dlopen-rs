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

    let f = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", f(1, 1));

    let f = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    f("dlopen-rs: hello world");

    let f = unsafe { libexample.get::<fn()>("thread_local").unwrap() };
    f();

    let f = unsafe { libexample.get::<fn()>("panic").unwrap() };
    f();
}
