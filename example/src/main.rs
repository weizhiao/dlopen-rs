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
    let path =
        Path::new("/home/wei/dlopen-rs/target/x86_64-unknown-linux-musl/release/libexample.so");

    let libc = MyLib(Arc::new(unsafe {
        Library::new("/lib/x86_64-linux-gnu/libc.so.6").unwrap()
    }));

    let libgcc = ELFLibrary::from_file("/usr/lib/llvm-19/lib/libunwind.so")
        .unwrap()
        .relocate_with::<MyLib>(&[], Some(libc.clone()))
        .unwrap();
    let libexample = ELFLibrary::from_file(path)
        .unwrap()
        .relocate_with::<MyLib>(&[libgcc], Some(libc))
        .unwrap();

    let f: dlopen_rs::Symbol<extern "C" fn(i32) -> i32> = libexample.get("c_fun_add_two").unwrap();
    println!("{}", f(2));

    let g: dlopen_rs::Symbol<extern "C" fn()> =
        libexample.get("c_fun_print_something_else").unwrap();
    g();

    let f: dlopen_rs::Symbol<extern "C" fn()> = libexample.get("c_test").unwrap();
    f();
}
