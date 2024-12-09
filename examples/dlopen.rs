use dlopen_rs::ElfLibrary;
use std::path::Path;

fn main() {
    std::env::set_var("LD_LIBRARY_PATH", "/lib");
    std::env::set_var("LD_BIND_NOW", "0");
    let path = Path::new("./target/release/libexample.so");
    let libexample = ElfLibrary::dlopen(path).unwrap();
    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");

    drop(libexample);

    let bytes = std::fs::read(path).unwrap();
    let libexample = ElfLibrary::dlopen_from_binary(&bytes, "example").unwrap();

    let backtrace = unsafe { libexample.get::<fn()>("backtrace").unwrap() };
    backtrace();
}
