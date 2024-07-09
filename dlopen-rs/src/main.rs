use std::path::Path;

use dlopen_rs::ELFLibrary;

fn main() {
    let path = Path::new("/home/wei/elf_loader/example-dylib/a.out");
    let lib = ELFLibrary::from_file(path).unwrap();
    let add = lib.get("add").unwrap();
    let add: extern "C" fn(i32, i32)->i32 = unsafe { core::mem::transmute(add) };
	println!("{}",add(1,1))
}
