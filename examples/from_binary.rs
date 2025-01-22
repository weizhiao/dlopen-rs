use dlopen_rs::{ElfLibrary, OpenFlags};
use std::path::Path;
fn main() {
	dlopen_rs::init();
    let path = Path::new("./target/release/libexample.so");

    let bytes = std::fs::read(path).unwrap();

    let libc = ElfLibrary::load_existing("libc.so.6").unwrap();
    let libgcc = ElfLibrary::load_existing("libgcc_s.so.1").unwrap();

    let libexample =
        ElfLibrary::from_binary(&bytes, "libexample.so", OpenFlags::CUSTOM_NOT_REGISTER)
            .unwrap()
            .relocate(&[libgcc,libc])
            .unwrap();

    let add: dlopen_rs::Symbol<fn(i32, i32) -> i32> = unsafe { libexample.get("add").unwrap() };
    println!("{}", add(1, 1));

    let print: dlopen_rs::Symbol<fn(&str)> = unsafe { libexample.get("print").unwrap() };
    print("dlopen-rs: hello world");

    let thread_local: dlopen_rs::Symbol<fn()> = unsafe { libexample.get("thread_local").unwrap() };
    thread_local();

    let panic: dlopen_rs::Symbol<fn()> = unsafe { libexample.get("panic").unwrap() };
    panic();
}
