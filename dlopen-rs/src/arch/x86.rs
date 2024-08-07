use elf::abi::*;

#[repr(C)]
pub(crate) struct TLSIndex {
    pub ti_module: usize,
    pub ti_offset: usize,
}

pub(crate) const TLS_DTV_OFFSET: usize = 0;

pub(crate) const REL_NONE: u32 = R_X86_64_NONE;
pub(crate) const REL_RELATIVE: u32 = R_X86_RELATIVE;
pub(crate) const REL_GOT: u32 = R_X86_64_GLOB_DAT;
pub(crate) const REL_DTPMOD: u32 = R_X86_64_DTPMOD64;
pub(crate) const REL_SYMBOLIC: u32 = R_X86_64_32;
pub(crate) const REL_JUMP_SLOT: u32 = R_X86_64_JUMP_SLOT;
pub(crate) const REL_TLSDESC: u32 = u32::MAX;