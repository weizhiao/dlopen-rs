use dlopen_rs::ELFLibrary;
use std::path::Path;
fn main() {
    let path = Path::new("./target/release/libexample.so");

    let bytes = std::fs::read(path).unwrap();

    let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();

    let libexample = ELFLibrary::from_binary(&bytes, "libexample.so")
        .unwrap()
        .relocate(&[libgcc, libc])
        .unwrap();

    let f: dlopen_rs::Symbol<extern "C" fn(i32) -> i32> =
        unsafe { libexample.get("c_fun_add_two").unwrap() };
    println!("{}", f(2));

    let f: dlopen_rs::Symbol<extern "C" fn()> =
        unsafe { libexample.get("c_fun_print_something_else").unwrap() };
    f();

    let f: dlopen_rs::Symbol<extern "C" fn()> =
        unsafe { libexample.get("c_func_thread_local").unwrap() };
    f();

    let f: dlopen_rs::Symbol<extern "C" fn()> = unsafe { libexample.get("c_func_panic").unwrap() };
    f();
}
