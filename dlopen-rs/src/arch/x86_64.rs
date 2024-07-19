use elf::abi::*;

pub(crate) const REL_RELATIVE: u32 = R_X86_64_RELATIVE;
pub(crate) const REL_GOT: u32 = R_X86_64_GLOB_DAT;
pub(crate) const REL_DTPMOD: u32 = R_X86_64_DTPMOD64;
pub(crate) const REL_SYMBOLIC: u32 = R_X86_64_64;
pub(crate) const REL_IRELATIVE: u32 = R_X86_64_IRELATIVE;
pub(crate) const REL_JUMP_SLOT: u32 = R_X86_64_JUMP_SLOT;
