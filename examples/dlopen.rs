use dlopen_rs::ELFLibrary;
use std::path::Path;

// 需要设置RUST_LD_LIBRARY_PATH环境变量，设置方法与LD_LIBRARY_PATH相同
fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libexample = ELFLibrary::dlopen(path).unwrap();
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
}
