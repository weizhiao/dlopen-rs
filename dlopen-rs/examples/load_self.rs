use std::path::Path;
use dlopen_rs::ELFLibrary;

fn main() {
    let path =
        Path::new("/home/wei/dlopen-rs/target/release/libexample.so");

    let libc = ELFLibrary::load_self("libc").unwrap();
    let libgcc = ELFLibrary::load_self("libgcc")
        .unwrap();

    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate(&[libgcc, libc])
        .unwrap();

    let f: dlopen_rs::Symbol<extern "C" fn(i32) -> i32> = libexample.get("c_fun_add_two").unwrap();
    println!("{}", f(2));

    let g: dlopen_rs::Symbol<extern "C" fn()> =
        libexample.get("c_fun_print_something_else").unwrap();
    g();

    let f: dlopen_rs::Symbol<extern "C" fn()> = libexample.get("c_func_thread_local").unwrap();
    f();
}
