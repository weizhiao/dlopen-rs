use dlopen_rs::{ELFLibrary, MmapImpl};
use std::path::Path;
fn main() {
    let path = Path::new("./target/release/libexample.so");

    let bytes = std::fs::read(path).unwrap();

    let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();

    let libexample = ELFLibrary::from_binary::<MmapImpl>(&bytes, "libexample.so")
        .unwrap()
        .relocate(&[libgcc, libc])
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
