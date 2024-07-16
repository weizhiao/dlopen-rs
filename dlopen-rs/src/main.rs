

// struct Dump {
//     hash_map: HashMap<String, *const ()>,
// }

// impl GetSymbol for Dump {
//     fn get_sym(&self, name: &str) -> Option<*const ()> {
//         self.hash_map.get(name)
//     }
// }

// extern "C" {
//     fn __cxa_finalize();
//     fn _ITM_registerTMCloneTable();
//     fn _ITM_deregisterTMCloneTable();
//     fn __gmon_start__();
// }

fn main() {
    // let path = Path::new("/home/wei/dlopen-rs/target/release/libexample.so");
    // let lib = ELFLibrary::from_file(path).unwrap();
    // let libgcc = ELFLibrary::from_file(Path::new("/lib/x86_64-linux-gnu/libgcc_s.so.1")).unwrap();
    // let libc = ELFLibrary::from_file(Path::new("/lib/x86_64-linux-gnu/libc.so.6")).unwrap();
    // let mut hash_map = HashMap::new();
    // hash_map.insert("__cxa_finalize".to_owned(), __cxa_finalize as *const ());
    // hash_map.insert("_ITM_registerTMCloneTable".to_owned(), 0 as *const ());
    // hash_map.insert("_ITM_deregisterTMCloneTable".to_owned(), 0 as *const ());
    // hash_map.insert("__gmon_start__".to_owned(), 0 as *const ());
    // let dump = Dump { hash_map };
    // lib.relocate_with(&[&libc, &libgcc], &[&dump]).unwrap();
    // let f = lib.get_sym("c_fun_print_something_else").unwrap();
    // let f: extern "C" fn() = unsafe { core::mem::transmute(f) };
    // f();
    // let add = lib.get_sym("add").unwrap();
    // let add: extern "C" fn(i32, i32) -> i32 = unsafe { core::mem::transmute(add) };

    // println!("{}", add(1, 1))
}
