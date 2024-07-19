#![feature(cfg_match)]
#![cfg_attr(feature = "nightly", core_intrinsics)]
mod arch;
mod builtin;
mod dynamic;
mod ehdr;
mod file;
mod handle;
mod hash;
mod loader;
mod relocation;
mod segment;
#[cfg(feature = "tls")]
mod tls;
mod unwind;

pub use handle::{ELFInstance, ELFLibrary};
pub use relocation::GetSymbol;

// 因为unlikely只能在nightly版本的编译器中使用
#[cfg(not(feature = "nightly"))]
use core::convert::identity as unlikely;
#[cfg(feature = "nightly")]
use core::intrinsics::unlikely;

use elf::file::Class;

extern crate alloc;

#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "aarch64",
    target_arch = "riscv32",
    target_arch = "riscv64",
    target_arch = "arm"
)))]
compile_error!("unsupport arch");

#[cfg(target_arch = "aarch64")]
const PAGE_SIZE: usize = 0x10000;
#[cfg(not(target_arch = "aarch64"))]
const PAGE_SIZE: usize = 0x1000;

const MASK: usize = (0 - PAGE_SIZE as isize) as usize;

#[cfg(target_pointer_width = "64")]
const E_CLASS: Class = Class::ELF64;
#[cfg(target_pointer_width = "32")]
const E_CLASS: Class = Class::ELF32;

#[cfg(target_pointer_width = "64")]
type Phdr = elf::segment::Elf64_Phdr;
#[cfg(target_pointer_width = "32")]
type Phdr = elf::segment::Elf32_Phdr;

#[cfg(target_pointer_width = "64")]
type Dyn = elf::dynamic::Elf64_Dyn;
#[cfg(target_pointer_width = "32")]
type Dyn = elf::dynamic::Elf32_Dyn;

#[cfg(target_pointer_width = "64")]
type Rela = elf::relocation::Elf64_Rela;
#[cfg(target_pointer_width = "32")]
type Rela = elf::relocation::Elf32_Rela;

#[cfg(target_pointer_width = "64")]
type Symbol = elf::symbol::Elf64_Sym;
#[cfg(target_pointer_width = "32")]
type Symbol = elf::symbol::Elf32_Sym;

#[cfg(target_pointer_width = "64")]
const REL_MASK: usize = 0xFFFFFFFF;
#[cfg(target_pointer_width = "32")]
const REL_MASK: usize = 0xFF;

#[cfg(target_pointer_width = "64")]
const REL_BIT: usize = 32;
#[cfg(target_pointer_width = "32")]
const REL_BIT: usize = 8;

#[cfg(target_pointer_width = "64")]
const PHDR_SIZE: usize = core::mem::size_of::<elf::segment::Elf64_Phdr>();
#[cfg(target_pointer_width = "32")]
const PHDR_SIZE: usize = core::mem::size_of::<elf::segment::Elf32_Phdr>();

#[cfg(target_pointer_width = "64")]
const EHDR_SIZE: usize = core::mem::size_of::<elf::file::Elf64_Ehdr>();
#[cfg(target_pointer_width = "32")]
const EHDR_SIZE: usize = core::mem::size_of::<elf::file::Elf32_Ehdr>();

#[cfg(feature = "std")]
const BUF_SIZE: usize = EHDR_SIZE + 8 * PHDR_SIZE;

use elf::parse::ParseError;
use snafu::prelude::*;

#[derive(Debug, Snafu)]
#[non_exhaustive]
pub enum Error {
    #[cfg(feature = "std")]
    IOError {
        source: std::io::Error,
    },
    #[snafu(display("Can't parse file, {msg}"))]
    ParseError {
        msg: ParseError,
    },
    #[snafu(display("Can't parse file, {msg}"))]
    GimliError {
        msg: gimli::Error,
    },
    #[snafu(display("Can't parse file, {msg}"))]
    LoaderError {
        msg: &'static str,
    },
    RelocateError {
        msg: String,
    },
    #[snafu(display("Unknown Error"))]
    Unknown,
    #[cfg(all(feature = "mmap", unix))]
    Errno {
        source: nix::Error,
    },
    ArchMismatch,
    ClassMismatch,
    FileTypeMismatch,
}

#[cold]
#[inline(never)]
fn parse_err_convert(err: elf::ParseError) -> Error {
    Error::ParseError { msg: err }
}

#[cold]
#[inline(never)]
fn gimli_err_convert(err: gimli::Error) -> Error {
    Error::GimliError { msg: err }
}

#[cold]
#[inline(never)]
fn elfloader_error<T>(msg: &'static str) -> Result<T> {
    Err(Error::LoaderError { msg })
}

pub type Result<T> = core::result::Result<T, Error>;
