use elf::abi::*;

#[repr(C)]
pub(crate) struct TLSIndex {
    pub ti_module: usize,
    pub ti_offset: usize,
}

/* Dynamic thread vector pointers point 0x800 past the start of each
TLS block.  */
pub(crate) const TLS_DTV_OFFSET: usize = 0x800;

pub(crate) const REL_NONE: u32 = R_RISCV_NONE;
pub(crate) const REL_RELATIVE: u32 = R_RISCV_RELATIVE;
// RISCV does not have this
pub(crate) const REL_GOT: u32 = u32::MAX;
pub(crate) const REL_DTPMOD: u32 = R_RISCV_TLS_DTPMOD64;
pub(crate) const REL_SYMBOLIC: u32 = R_RISCV_64;
pub(crate) const REL_JUMP_SLOT: u32 = R_RISCV_JUMP_SLOT;
pub(crate) const REL_TLSDESC: u32 = u32::MAX;
