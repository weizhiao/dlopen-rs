use dlopen_rs::{ElfLibrary, OpenFlags};
use libc::{c_void, size_t};
use std::path::Path;

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

    let libexample1 = ElfLibrary::from_file(path, OpenFlags::CUSTOM_NOT_REGISTER)
        .unwrap()
        .relocate_with(&[libc.clone(), libgcc.clone()], &|name: &str| {
            if name == "malloc" {
                return Some(mymalloc as _);
            } else {
                return None;
            }
        })
        .unwrap();

    let add = unsafe { libexample1.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample1.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");

    let thread_local = unsafe { libexample1.get::<fn()>("thread_local").unwrap() };
    thread_local();

    drop(libexample1);

    let bytes = std::fs::read(path).unwrap();
    let libexample2 = ElfLibrary::from_binary(
        &bytes,
        path.as_os_str().to_str().unwrap(),
        OpenFlags::RTLD_GLOBAL,
    )
    .unwrap()
    .relocate(&[libgcc, libc])
    .unwrap();

    let panic = unsafe { libexample2.get::<fn()>("panic").unwrap() };
    panic();

    let backtrace = unsafe { libexample2.get::<fn()>("backtrace").unwrap() };
    backtrace();
}
