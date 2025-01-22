use dlopen_rs::{ElfLibrary, OpenFlags};
use std::path::Path;

fn main() {
    std::env::set_var("RUST_LOG", "debug");
    env_logger::init();
    dlopen_rs::init();
    let path = Path::new("./target/release/libexample.so");
    let libexample = ElfLibrary::dlopen(path, OpenFlags::RTLD_GLOBAL|OpenFlags::RTLD_NOW).unwrap();
    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");

    // drop(libexample);

    // let bytes = std::fs::read(path).unwrap();
    // let libexample = ElfLibrary::dlopen_from_binary(&bytes, "example").unwrap();

    let backtrace = unsafe { libexample.get::<fn()>("backtrace").unwrap() };
    backtrace();

    let temp = ElfLibrary::dlopen(
        "/lib/llvm-18/lib/libLLVM-18.so",
        OpenFlags::RTLD_LOCAL | OpenFlags::RTLD_NOW,
    )
    .unwrap();
}
