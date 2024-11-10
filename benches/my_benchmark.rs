use std::path::Path;

use criterion::{criterion_group, criterion_main, Criterion};
use dlopen_rs::{ELFLibrary, MmapImpl};
use libloading::Library;

fn criterion_benchmark(c: &mut Criterion) {
    let path = Path::new("./target/release/libexample.so");
    let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
    let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();
    c.bench_function("dlopen-rs", |b| {
        b.iter(|| {
            let _libexample = ELFLibrary::from_file::<MmapImpl>(path)
                .unwrap()
                .relocate(&[libc.clone(), libgcc.clone()])
                .finish()
                .unwrap();
        })
    });
    c.bench_function("dlopen", |b| {
        b.iter(|| {
            unsafe { Library::new(path).unwrap() };
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
