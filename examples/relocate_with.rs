use dlopen_rs::{ElfLibrary, OpenFlags};
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

    let func = move |name: &str| {
        if name == "malloc" {
            return Some(mymalloc as _);
        } else {
            return None;
        }
    };

    let libexample = ElfLibrary::from_file(path, OpenFlags::CUSTOM_NOT_REGISTER)
        .unwrap()
        .relocate_with(&[libc, libgcc], &func)
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
