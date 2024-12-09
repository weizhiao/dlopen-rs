use std::path::Path;

use criterion::{criterion_group, criterion_main, Criterion};
use dlopen_rs::ElfLibrary;
use libloading::Library;

fn load_benchmark(c: &mut Criterion) {
    std::env::set_var("LD_LIBRARY_PATH", "/lib");
    let path = Path::new("./target/release/libexample.so");
    let libc = ElfLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ElfLibrary::sys_load("libgcc_s.so.1").unwrap();
    c.bench_function("dlopen-rs:from_file", |b| {
        b.iter(|| {
            let _libexample = ElfLibrary::from_file(path, None)
                .unwrap()
                .relocate(&[libc.clone(), libgcc.clone()])
                .finish()
                .unwrap();
        });
    });
    c.bench_function("dlopen-rs:dlopen", |b| {
        b.iter(|| {
            let _libexample = ElfLibrary::dlopen(path);
        })
    });
    c.bench_function("libloading:new", |b| {
        b.iter(|| {
            unsafe { Library::new(path).unwrap() };
        })
    });
}

fn get_symbol_benchmark(c: &mut Criterion) {
    std::env::set_var("LD_LIBRARY_PATH", "/lib");
    let path = Path::new("./target/release/libexample.so");
    let lib1 = ElfLibrary::dlopen(path).unwrap();
    let lib2 = unsafe { Library::new(path).unwrap() };
    c.bench_function("dlopen-rs:get", |b| {
        b.iter(|| unsafe { lib1.get::<fn(i32, i32) -> i32>("add").unwrap() })
    });
    c.bench_function("libloading:get", |b| {
        b.iter(|| {
            unsafe { lib2.get::<fn(i32, i32) -> i32>("add".as_bytes()).unwrap() };
        })
    });
}

criterion_group!(benches, load_benchmark, get_symbol_benchmark);
criterion_main!(benches);
