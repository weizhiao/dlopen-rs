use std::path::Path;

use dlopen_rs::{ELFLibrary, Mmap};

struct MyMmapImpl;

impl Mmap for MyMmapImpl {
    unsafe fn mmap(
        addr: Option<usize>,
        len: usize,
        prot: dlopen_rs::ProtFlags,
        flags: dlopen_rs::MapFlags,
        offset: dlopen_rs::Offset,
    ) -> dlopen_rs::Result<std::ptr::NonNull<std::ffi::c_void>> {
        todo!()
    }

    unsafe fn mmap_anonymous(
        addr: usize,
        len: usize,
        prot: dlopen_rs::ProtFlags,
        flags: dlopen_rs::MapFlags,
    ) -> dlopen_rs::Result<std::ptr::NonNull<std::ffi::c_void>> {
        todo!()
    }

    unsafe fn munmap(
        addr: std::ptr::NonNull<std::ffi::c_void>,
        len: usize,
    ) -> dlopen_rs::Result<()> {
        todo!()
    }

    unsafe fn mprotect(
        addr: std::ptr::NonNull<std::ffi::c_void>,
        len: usize,
        prot: dlopen_rs::ProtFlags,
    ) -> dlopen_rs::Result<()> {
        todo!()
    }
}

fn main() {
    let path = Path::new("./target/release/libexample.so");
    let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();

    let libexample = ELFLibrary::from_file::<MyMmapImpl>(path)
        .unwrap()
        .relocate(&[libc, libgcc])
        .finish()
        .unwrap();

    let add = unsafe { libexample.get::<fn(i32, i32) -> i32>("add").unwrap() };
    println!("{}", add(1, 1));

    let print = unsafe { libexample.get::<fn(&str)>("print").unwrap() };
    print("dlopen-rs: hello world");
}
