use libloading::Library;
use std::{fs, path::Path, time::Instant};

#[test]
fn load() {
    unsafe { std::env::set_var("LD_BIND_NOW", "1") };
    let path = Path::new("/usr/lib/x86_64-linux-gnu/");
    if !path.is_dir() {
        panic!("input path must be a directory")
    }
    let skip_libs = ["libeatmydata.so", "libmemusage.so", "Ubuntu.so"];
    let is_load = |s: &str| {
        for name in skip_libs {
            if s.contains(name) {
                return false;
            }
        }
        return true;
    };

    let start = Instant::now();
    // 遍历目录中的所有文件
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();

        // 检查文件后缀是否为 `.so`
        if path.extension().and_then(|ext| ext.to_str()) == Some("so") {
            let s = path.to_str().unwrap();
            if !is_load(s) {
                continue;
            }
            println!("dlopen {:?}", path);
            if let Err(err) = unsafe { Library::new(&path) } {
                eprintln!("dlopen {:?} failed: {}", path, err);
            }
        }
    }
    let end = Instant::now().duration_since(start);
    println!("{:?}", end);
}
