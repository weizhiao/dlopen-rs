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

    let f: dlopen_rs::Symbol<fn(i32, i32) -> i32> = unsafe { libexample.get("add").unwrap() };
    println!("{}", f(1, 1));

    let g: dlopen_rs::Symbol<fn(&str)> = unsafe { libexample.get("print").unwrap() };
    g("dlopen-rs: hello world");

    let f: dlopen_rs::Symbol<fn()> = unsafe { libexample.get("thread_local").unwrap() };
    f();

    let f: dlopen_rs::Symbol<fn()> = unsafe { libexample.get("panic").unwrap() };
    f();
}
