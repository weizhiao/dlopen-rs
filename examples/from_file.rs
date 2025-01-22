use dlopen_rs::{ElfLibrary, OpenFlags};
use std::path::Path;

fn main() {
	std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    dlopen_rs::init();
    let path = Path::new("./target/release/libexample.so");
    let libc = ElfLibrary::load_existing("libc.so.6").unwrap();
    let libgcc = ElfLibrary::load_existing("libgcc_s.so.1").unwrap();

    let libexample = ElfLibrary::from_file(path, OpenFlags::RTLD_LOCAL | OpenFlags::RTLD_LAZY)
        .unwrap()
        .relocate(&[libc, libgcc])
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
