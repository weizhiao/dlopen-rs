use core::{ffi::c_void, ops::Range};
use elf_loader::Unwind;

#[derive(Clone)]
pub(crate) struct EhFrame(usize);

impl Unwind for EhFrame {
    unsafe fn new(phdr: &elf_loader::arch::Phdr, map_range: Range<usize>) -> Option<Self> {
        let eh_frame_hdr_off = phdr.p_vaddr as usize;
        let eh_frame_hdr_size = phdr.p_memsz as usize;
        let bases = gimli::BaseAddresses::default()
            .set_eh_frame_hdr((eh_frame_hdr_off + map_range.start) as _);
        let eh_frame_hdr = gimli::EhFrameHdr::new(
            core::slice::from_raw_parts(
                (map_range.start + eh_frame_hdr_off) as *const u8,
                eh_frame_hdr_size,
            ),
            gimli::NativeEndian,
        )
        .parse(&bases, core::mem::size_of::<usize>() as _)
        .unwrap();
        let eh_frame_addr = match eh_frame_hdr.eh_frame_ptr() {
            gimli::Pointer::Direct(x) => x as usize,
            gimli::Pointer::Indirect(x) => unsafe { *(x as *const _) },
        };
        let unwind = EhFrame(eh_frame_addr);

        extern "C" {
            fn __register_frame(begin: *const c_void);
        }
        //在使用libgcc的情况下直接传eh_frame的地址即可
        unsafe { __register_frame(eh_frame_addr as _) };
        Some(unwind)
    }
}

impl Drop for EhFrame {
    fn drop(&mut self) {
        extern "C" {
            fn __deregister_frame(begin: *const c_void);
        }
        unsafe { __deregister_frame(self.0 as _) };
    }
}
