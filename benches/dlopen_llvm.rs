use criterion::{criterion_group, criterion_main, Criterion};
use dlopen_rs::{ElfLibrary, OpenFlags};
use std::path::Path;

fn load(c: &mut Criterion) {
    dlopen_rs::init();
    let path = Path::new("/usr/lib/llvm-19/lib/libLLVM.so");
    c.bench_function("dlopen-rs:dlopen", |b| {
        b.iter(|| {
            let _libexample = ElfLibrary::dlopen(path, OpenFlags::RTLD_GLOBAL).unwrap();
        })
    });
}

criterion_group!(benches, load);
criterion_main!(benches);
