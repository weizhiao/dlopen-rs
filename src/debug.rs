use core::{
    ffi::{c_char, c_int, c_void},
    ptr::{addr_of_mut, null_mut},
};
use std::sync::{Mutex, Once};

use elf_loader::arch::Dyn;

const RT_ADD: c_int = 1;
const RT_CONSISTENT: c_int = 0;
const RT_DELETE: c_int = 2;

// struct link_map {
// 	ElfW(Addr) l_addr;
// 	char *l_name;
// 	ElfW(Dyn) *l_ld;
// 	struct link_map *l_next, *l_prev;
// };
#[repr(C)]
struct LinkMap {
    pub l_addr: *mut c_void,
    pub l_name: *const c_char,
    pub l_ld: *mut Dyn,
    l_next: *mut LinkMap,
    l_prev: *mut LinkMap,
}

#[repr(C)]
struct Debug {
    version: c_int,
    map: *mut LinkMap,
    brk: extern "C" fn(),
    state: c_int,
    ldbase: *mut c_void,
}

struct CustomDebug {
    debug: *mut Debug,
    tail: *mut LinkMap,
}

unsafe impl Sync for CustomDebug {}
unsafe impl Send for CustomDebug {}

pub(crate) struct DebugInfo {
    link_map: Box<LinkMap>,
}

impl Drop for DebugInfo {
    fn drop(&mut self) {
        unsafe {
            let mut custom_debug = DEBUG.lock().unwrap();
            let tail = custom_debug.tail;
            let debug = &mut *custom_debug.debug;
            debug.state = RT_DELETE;
            (debug.brk)();
            match (
                debug.map == self.link_map.as_mut(),
                tail == self.link_map.as_mut(),
            ) {
                (true, true) => {
                    debug.map = null_mut();
                    custom_debug.tail = null_mut();
                }
                (true, false) => {
                    debug.map = self.link_map.l_next;
                    (*self.link_map.l_next).l_prev = null_mut();
                }
                (false, true) => {
                    let prev = &mut *self.link_map.l_prev;
                    prev.l_next = null_mut();
                    custom_debug.tail = prev;
                }
                (false, false) => {
                    let prev = &mut *self.link_map.l_prev;
                    let next = &mut *self.link_map.l_next;
                    prev.l_next = next;
                    next.l_prev = prev;
                }
            }
            debug.state = RT_CONSISTENT;
            (debug.brk)();
        }
    }
}

extern "C" {
    static mut _r_debug: Debug;
}

static DEBUG: Mutex<CustomDebug> = Mutex::new(CustomDebug {
    debug: null_mut(),
    tail: null_mut(),
});

impl DebugInfo {
    pub(crate) unsafe fn new(base: usize, name: *const i8, dynamic: usize) -> DebugInfo {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let debug = unsafe { &mut *addr_of_mut!(_r_debug) };
            let prev = if let Some(head) = debug.map.as_mut() {
                //第一个是程序本身
                debug.map = Box::leak(Box::new(LinkMap {
                    l_addr: head.l_addr,
                    l_name: head.l_name,
                    l_ld: head.l_ld,
                    l_next: null_mut(),
                    l_prev: null_mut(),
                }));
                debug.map
            } else {
                null_mut()
            };

            let mut custom = DEBUG.lock().unwrap();
            custom.debug = debug;
            custom.tail = prev;
        });
        let mut custom_debug = DEBUG.lock().unwrap();
        let tail = custom_debug.tail;
        let debug = &mut *custom_debug.debug;
        let link_map = Box::leak(Box::new(LinkMap {
            l_addr: base as _,
            l_name: name,
            l_ld: dynamic as _,
            l_next: null_mut(),
            l_prev: tail,
        }));
        if tail.is_null() {
            debug.map = link_map;
        } else {
            (*tail).l_next = link_map;
        }
        custom_debug.tail = link_map;
        debug.state = RT_ADD;
        (debug.brk)();
        debug.state = RT_CONSISTENT;
        (debug.brk)();
        DebugInfo {
            link_map: Box::from_raw(link_map),
        }
    }
}
