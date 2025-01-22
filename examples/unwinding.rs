use dlopen_rs::{ElfLibrary, OpenFlags};
use std::path::Path;

fn main() {
	dlopen_rs::init();
    let path = Path::new("./target/release/libexample.so");

    let libc = ElfLibrary::load_existing("libc.so.6").unwrap();
    let libexample = ElfLibrary::from_file(path, OpenFlags::CUSTOM_NOT_REGISTER)
        .unwrap()
        .relocate(&[libc])
        .unwrap();

    let f: dlopen_rs::Symbol<fn(i32, i32) -> i32> = unsafe { libexample.get("add").unwrap() };
    println!("{}", f(1, 1));

    let g: dlopen_rs::Symbol<fn(&str)> = unsafe { libexample.get("print").unwrap() };
    g("dlopen-rs: hello world");

    let f: dlopen_rs::Symbol<fn()> = unsafe { libexample.get("thread_local").unwrap() };
    f();

    let f: dlopen_rs::Symbol<fn()> = unsafe { libexample.get("panic").unwrap() };
    f();
}
