use dlopen_rs::{ELFLibrary, MmapImpl};
use std::path::Path;

fn main() {
    std::env::set_var("RUST_LD_LIBRARY_PATH", "/lib32");
    let path = Path::new("./target/release/libexample.so");
    let libexample = ELFLibrary::dlopen::<MmapImpl>(path).unwrap();
    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");
}
