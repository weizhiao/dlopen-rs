use dlopen_rs::{ElfLibrary, Register};
use std::path::Path;

fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libc = ElfLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ElfLibrary::sys_load("libgcc_s.so.1").unwrap();

    let libexample = ElfLibrary::from_file(path)
        .unwrap()
        .register()
        .relocate(&[libc])
        .relocate(&[libgcc])
        .finish()
        .unwrap();

    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");

    let thread_local = unsafe { libexample.get::<fn()>("thread_local").unwrap() };
    thread_local();

    let panic = unsafe { libexample.get::<fn()>("panic").unwrap() };
    panic();

    let backtrace = unsafe { libexample.get::<fn()>("backtrace").unwrap() };
    backtrace();
}
