//!An example dynamically loadable library.
//!
//! This crate creates a dynamic library that can be used for testing purposes.

use std::{backtrace::Backtrace, cell::Cell, thread};

#[no_mangle]
pub fn panic() {
    let res = std::panic::catch_unwind(|| {
        panic!("panic!");
    });
    assert!(res.is_err());
    println!("catch panic!")
}

thread_local! {
    static NUM:Cell<i32>=Cell::new(0)
}

#[no_mangle]
pub fn backtrace() {
    println!("{}", Backtrace::force_capture());
}

#[no_mangle]
pub fn thread_local() {
    println!("{}", HELLO);
    let handle = thread::spawn(|| {
        NUM.set(NUM.get() + 1);
        println!("thread1:{}", NUM.get());
    });
    handle.join().unwrap();
    NUM.set(NUM.get() + 2);
    println!("thread2:{}", NUM.get());
}

#[no_mangle]
pub fn print(str: &str) {
    println!("{}", str);
}

#[no_mangle]
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[no_mangle]
pub static HELLO: &str = "Hello!";
