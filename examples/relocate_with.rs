use dlopen_rs::ElfLibrary;
use nix::libc::size_t;
use std::{ffi::c_void, path::Path};

extern "C" fn mymalloc(size: size_t) -> *mut c_void {
    println!("malloc:{}bytes", size);
    unsafe { nix::libc::malloc(size) }
}

fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libc = ElfLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ElfLibrary::sys_load("libgcc_s.so.1").unwrap();

    let libexample = ElfLibrary::from_file(path)
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

    let f = unsafe { libexample.get::<fn()>("thread_local").unwrap() };
    f();

    let f = unsafe { libexample.get::<fn()>("panic").unwrap() };
    f();
}
