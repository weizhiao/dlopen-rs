#[cfg(all(feature = "mmap_impl", feature = "no_mmap_impl"))]
compile_error!("The \"mmap_impl\" and \"no_mmap_impl\" feature cannot be enabled at the same time");

cfg_if::cfg_if! {
    if #[cfg(feature = "mmap_impl")]{
        pub(crate) mod mmap;
        pub use mmap::MmapImpl;
    }else if #[cfg(feature = "no_mmap_impl")]{
        pub(crate) mod no_mmap;
        pub use no_mmap::MmapImpl;
    }else{
        #[allow(unused_variables)]
        pub(crate) mod dummy;
        pub use dummy::MmapImpl;
    }
}

use crate::Result;
use bitflags::bitflags;
use core::{
    ffi::{c_int, c_void},
    ptr::NonNull,
};

bitflags! {
    #[derive(Clone, Copy)]
    /// Desired memory protection of a memory mapping.
    pub struct ProtFlags: c_int {
        /// Pages cannot be accessed.
        const PROT_NONE = 0;
        /// Pages can be read.
        const PROT_READ = 1;
        /// Pages can be written.
        const PROT_WRITE = 2;
        /// Pages can be executed
        const PROT_EXEC = 4;
    }
}

bitflags! {
     /// Additional parameters for [`mmap`].
     pub struct MapFlags: c_int {
        /// Create a private copy-on-write mapping. Mutually exclusive with `MAP_SHARED`.
        const MAP_PRIVATE = 2;
        /// Place the mapping at exactly the address specified in `addr`.
        const MAP_FIXED = 16;
        /// The mapping is not backed by any file.
        const MAP_ANONYMOUS = 32;
    }
}

pub enum OffsetType {
    File { fd: c_int, file_offset: usize },
    Addr(*const u8),
}

pub struct Offset {
    pub offset: usize,
    pub len: usize,
    pub kind: OffsetType,
}

pub trait RawData {
    fn transport(&self, offset: usize, len: usize) -> Offset;
}

pub trait Mmap {
    unsafe fn mmap(
        addr: Option<usize>,
        len: usize,
        prot: ProtFlags,
        flags: MapFlags,
        offset: Offset,
    ) -> Result<NonNull<c_void>>;

    unsafe fn mmap_anonymous(
        addr: usize,
        len: usize,
        prot: ProtFlags,
        flags: MapFlags,
    ) -> Result<NonNull<c_void>>;

    unsafe fn mummap(addr: NonNull<c_void>, len: usize) -> Result<()>;

    unsafe fn mprotect(addr: NonNull<c_void>, len: usize, prot: ProtFlags) -> Result<()>;
}
