use dlopen_rs::{ELFLibrary, ExternLibrary};
use libloading::Library;
use std::{path::Path, sync::Arc};

#[derive(Debug, Clone)]
struct MyLib(Arc<Library>);

impl ExternLibrary for MyLib {
    fn get_sym(&self, name: &str) -> Option<*const ()> {
        let sym: Option<*const ()> = unsafe {
            self.0
                .get::<*const usize>(name.as_bytes())
                .map_or(None, |sym| Some(sym.into_raw().into_raw() as _))
        };
        sym
    }
}

fn main() {
    let path = Path::new("./target/release/libexample.so");

    let libc = MyLib(Arc::new(unsafe {
        Library::new("/lib/x86_64-linux-gnu/libc.so.6").unwrap()
    }));

    let libgcc = MyLib(Arc::new(unsafe {
        Library::new("/lib/x86_64-linux-gnu/libgcc_s.so.1").unwrap()
    }));

    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate_with(&[], vec![libc, libgcc])
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
