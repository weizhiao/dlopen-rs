use dlopen_rs::{ELFLibrary, GetSymbol};
use libloading::Library;
use std::path::Path;

struct MyLib(Library);

impl GetSymbol for MyLib {
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
    let path = Path::new("/home/wei/dlopen-rs/target/release/libexample.so");
    let libexample = ELFLibrary::from_file(path).unwrap();

    let libc = MyLib(unsafe { Library::new("/lib/x86_64-linux-gnu/libc.so.6").unwrap() });

    let libgcc = ELFLibrary::from_file("/lib/x86_64-linux-gnu/libgcc_s.so.1").unwrap();
    let libgcc = libgcc.relocate_with(&[], &[], &[&libc]).unwrap();
    let libexample = libexample.relocate_with(&[], &[&libgcc], &[&libc]).unwrap();

    let f = libexample.get_sym("c_fun_add_two").unwrap();
    let f: extern "C" fn(i32) -> i32 = unsafe { core::mem::transmute(f) };
    println!("{}", f(2));
    let g = libexample.get_sym("c_fun_print_something_else").unwrap();
    let g: extern "C" fn() = unsafe { core::mem::transmute(g) };
    g();
    let f = libexample.get_sym("c_test").unwrap();
    let f: extern "C" fn() = unsafe { core::mem::transmute(f) };
    f();
}
