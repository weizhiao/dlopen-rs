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

    let f = unsafe { libexample.get::<extern "C" fn()>("backtrace").unwrap() };
    f();
}
