use dlopen_rs::{ElfLibrary, OpenFlags};
use std::path::Path;

fn main() {
    std::env::set_var("RUST_LOG", "trace");
    env_logger::init();
    dlopen_rs::init();
    let path = Path::new("./target/release/libexample.so");
    let libexample1 =
        ElfLibrary::dlopen(path, OpenFlags::RTLD_LOCAL | OpenFlags::RTLD_LAZY).unwrap();
    let add = unsafe { libexample1.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample1.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");

    let args = unsafe { libexample1.get::<fn()>("args") }.unwrap();
    args();

    drop(libexample1);

    let bytes = std::fs::read(path).unwrap();
    let libexample2 = ElfLibrary::dlopen_from_binary(
        &bytes,
        "./target/release/libexample.so",
        OpenFlags::RTLD_GLOBAL,
    )
    .unwrap();

    let backtrace = unsafe { libexample2.get::<fn()>("backtrace").unwrap() };
    backtrace();
}
