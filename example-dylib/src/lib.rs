//!An example dynamically loadable library.
//!
//! This crate creates a dynamic library that can be used for testing purposes.
//! It exports multiple symbols with different types and abis.
//! It's main purpose is to be used in tests of dynlib crate.

use std::{
    backtrace::Backtrace,
    cell::Cell,
    ffi::CStr,
    os::raw::{c_char, c_int},
    thread,
};

#[no_mangle]
pub extern "C-unwind" fn c_func_panic() {
    let res = std::panic::catch_unwind(|| {
        println!("hello");
        panic!("panic!");
    });
    assert!(res.is_err());
    println!("catch panic!")
}

thread_local! {
    static NUM:Cell<i32>=Cell::new(0)
}

#[no_mangle]
pub extern "C" fn backtrace() {
    println!("{}", Backtrace::force_capture());
}

#[no_mangle]
pub extern "C" fn c_func_thread_local() {
    let handle = thread::spawn(|| {
        NUM.set(NUM.get() + 1);
        println!("thread1:{}", NUM.get());
    });
    handle.join().unwrap();
    NUM.set(NUM.get() + 2);
    println!("thread2:{}", NUM.get());
}

//FUNCTIONS
#[no_mangle]
pub fn rust_fun_print_something() {
    println!("something");
}

#[no_mangle]
pub fn rust_fun_add_one(arg: i32) -> i32 {
    arg + 1
}

#[no_mangle]
pub extern "C" fn c_fun_print_something_else() {
    println!("something else");
    let cstr = unsafe { CStr::from_ptr(c"rust1223".as_ptr()) };
    println!("{}", cstr.to_bytes().len());
}

#[no_mangle]
pub extern "C" fn c_fun_add_two(arg: c_int) -> c_int {
    arg + 2
}

#[allow(unused_variables)]
#[no_mangle]
pub extern "C" fn c_fun_variadic(txt: *const c_char) {
    //pretend to be variadic - impossible to do in Rust code
}

//STATIC DATA
#[no_mangle]
pub static mut rust_i32_mut: i32 = 42;
#[no_mangle]
pub static rust_i32: i32 = 43;

#[no_mangle]
pub static mut c_int_mut: c_int = 44;
#[no_mangle]
pub static c_int: c_int = 45;

#[repr(C)]
pub struct SomeData {
    first: c_int,
    second: c_int,
}

#[no_mangle]
pub static c_struct: SomeData = SomeData {
    first: 1,
    second: 2,
};

//STATIC STRINGS

//exporting str directly is not so easy - it is not Sized!
//you can only export a reference to str and this requires double dereference
#[no_mangle]
pub static rust_str: &str = "Hello!";

#[no_mangle]
pub static c_const_char_ptr: [u8; 4] = [b'H', b'i', b'!', 0];
