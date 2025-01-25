use libloading::Library;

fn main() {
    let libexample = unsafe { Library::new("./target/release/libexample.so").unwrap() };
    let add = unsafe {
        libexample
            .get::<fn(i32, i32) -> i32>("add".as_bytes())
            .unwrap()
    };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print".as_bytes()).unwrap() };
    print("dlopen-rs: hello world");

    let thread_local = unsafe { libexample.get::<fn()>("thread_local".as_bytes()).unwrap() };
    thread_local();

    let panic = unsafe { libexample.get::<fn()>("panic".as_bytes()).unwrap() };
    panic();

    let backtrace = unsafe { libexample.get::<fn()>("backtrace".as_bytes()).unwrap() };
    backtrace();
}
