use dlopen_rs::ELFLibrary;
use std::path::Path;

fn main() {
    let path = Path::new("./target/release/libexample.so");

    let libc = ELFLibrary::sys_load("libc.so.6").unwrap();

    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate(&[libc])
        .unwrap();

    let f: dlopen_rs::Symbol<extern "C" fn(i32) -> i32> =
        unsafe { libexample.get("c_fun_add_two").unwrap() };
    println!("{}", f(2));

    let g: dlopen_rs::Symbol<extern "C" fn()> =
        unsafe { libexample.get("c_fun_print_something_else").unwrap() };
    g();

    let f: dlopen_rs::Symbol<extern "C" fn()> =
        unsafe { libexample.get("c_func_thread_local").unwrap() };
    f();

    let f: dlopen_rs::Symbol<extern "C" fn()> = unsafe { libexample.get("c_func_panic").unwrap() };
    f();
}
