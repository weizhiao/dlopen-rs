use dlopen_rs::ELFLibrary;
use std::path::Path;

fn main() {
    let path = Path::new("./target/release/libexample.so");

    let libc = ELFLibrary::load_self("libc.so.6").unwrap();
    let libgcc = ELFLibrary::load_self("libgcc_s.so.1").unwrap();

    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate(&[libgcc, libc])
        .unwrap();

    // let lib1 = ELFLibrary::from_file("/home/wei/dlopen-rs/test.so")
    //     .unwrap()
    //     .relocate(&[])
    //     .unwrap();

    // let f = unsafe { lib1.get::<extern "C" fn() -> i32>("get_tls_var").unwrap() };

    // println!("{}", f());

    let f = unsafe {
        libexample
            .get::<extern "C" fn(i32) -> i32>("c_fun_add_two")
            .unwrap()
    };
    println!("{}", f(2));

    let f = unsafe {
        libexample
            .get::<extern "C" fn()>("c_fun_print_something_else")
            .unwrap()
    };
    f();

    let f = unsafe {
        libexample
            .get::<extern "C" fn()>("c_func_thread_local")
            .unwrap()
    };
    f();

    let f = unsafe { libexample.get::<extern "C" fn()>("c_func_panic").unwrap() };
    f();

    // let f = unsafe { libexample.get::<extern "C" fn()>("backtrace").unwrap() };
    // f();
}
