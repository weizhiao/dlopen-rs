#[cfg(feature = "libgcc")]
mod libgcc;
#[cfg(feature = "libunwind")]
mod libunwind;
#[cfg(feature = "unwinding")]
mod unwinding;

use crate::{segment::ELFSegments, Phdr, Result};

#[derive(Debug)]
pub(crate) struct ELFUnwind(usize);

#[cfg(not(feature = "unwinding"))]
impl ELFUnwind {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> Result<ELFUnwind> {
        use crate::gimli_err_convert;
        let base = segments.base();
        let eh_frame_hdr_off = phdr.p_vaddr as usize;
        let eh_frame_hdr_size = phdr.p_memsz as usize;
        let bases =
            gimli::BaseAddresses::default().set_eh_frame_hdr((eh_frame_hdr_off + base) as _);
        let eh_frame_hdr = gimli::EhFrameHdr::new(
            &segments.as_mut_slice()[eh_frame_hdr_off..eh_frame_hdr_off + eh_frame_hdr_size],
            gimli::NativeEndian,
        )
        .parse(&bases, core::mem::size_of::<usize>() as _)
        .map_err(gimli_err_convert)?;
        let eh_frame_addr = match eh_frame_hdr.eh_frame_ptr() {
            gimli::Pointer::Direct(x) => x as usize,
            gimli::Pointer::Indirect(x) => unsafe { *(x as *const _) },
        };
        Ok(ELFUnwind(eh_frame_addr))
    }
}

#[cfg(feature = "unwinding")]
impl ELFUnwind {
    pub(crate) fn new(phdr: &Phdr, segments: &ELFSegments) -> Result<ELFUnwind> {
        let eh_frame_hdr = segments.base() + phdr.p_vaddr as usize;
        Ok(ELFUnwind(eh_frame_hdr))
    }
}
