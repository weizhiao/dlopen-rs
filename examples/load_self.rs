use dlopen_rs::ELFLibrary;
use std::path::Path;

fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();

    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate(&[libc, libgcc])
        .unwrap();

    libexample.register();

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
