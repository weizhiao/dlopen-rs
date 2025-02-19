use criterion::{criterion_group, criterion_main, Criterion};
use dlopen_rs::{ElfLibrary, OpenFlags};
use libloading::Library;
use std::path::Path;

fn load(c: &mut Criterion) {
    dlopen_rs::init();
    let path = Path::new("./target/release/libexample.so");
    let libc = ElfLibrary::load_existing("libc.so.6").unwrap();
    let libgcc = ElfLibrary::load_existing("libgcc_s.so.1").unwrap();
    c.bench_function("dlopen-rs:from_file", |b| {
        b.iter(|| {
            let _libexample = ElfLibrary::from_file(path, OpenFlags::CUSTOM_NOT_REGISTER)
                .unwrap()
                .relocate(&[libc.clone(), libgcc.clone()])
                .unwrap();
        });
    });
    c.bench_function("dlopen-rs:dlopen", |b| {
        b.iter(|| {
            let _libexample = ElfLibrary::dlopen(path, OpenFlags::RTLD_GLOBAL).unwrap();
        })
    });
    c.bench_function("libloading:new", |b| {
        b.iter(|| {
            unsafe { Library::new(path).unwrap() };
        })
    });
}

criterion_group!(benches, load);
criterion_main!(benches);
