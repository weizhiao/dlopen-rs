//! The `elf` crate provides a pure-safe-rust interface for reading ELF object files.
//! The `dopen_rs` crate supports loading dynamic libraries from memory and files,
//! supports `no_std` environments, and does not rely on the dynamic linker `ldso`
//!
//! There is currently no support for using backtrace in loaded dynamic library code,
//! and there is no support for debugging loaded dynamic libraries using gdb
//! 
//! # Examples
//! ```
//! use dlopen_rs::ELFLibrary;
//! use std::path::Path;
//! let path = Path::new("./target/release/libexample.so");
//! let libc = ELFLibrary::load_self("libc").unwrap();
//! let libgcc = ELFLibrary::load_self("libgcc").unwrap();
//! let libexample = ELFLibrary::from_file(path)
//!		.unwrap()
//!		.relocate(&[libgcc, libc])
//!		.unwrap();
//!
//! let f: dlopen_rs::Symbol<extern "C" fn(i32) -> i32> =
//! 	unsafe { libexample.get("c_fun_add_two").unwrap() };
//! println!("{}", f(2));
//! let f: dlopen_rs::Symbol<extern "C" fn()> =
//! 	unsafe { libexample.get("c_fun_print_something_else").unwrap() };
//! f();
//! let f: dlopen_rs::Symbol<extern "C" fn()> =
//! 	unsafe { libexample.get("c_func_thread_local").unwrap() };
//! f();
//! let f: dlopen_rs::Symbol<extern "C" fn()> =
//! 	unsafe { libexample.get("c_func_panic").unwrap() };
//! f();
//! ```
#![cfg_attr(feature = "nightly", allow(internal_features))]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

mod arch;
mod builtin;
mod dynamic;
mod ehdr;
mod file;
mod hashtable;
#[cfg(feature = "load_self")]
mod load_self;
mod loader;
mod relocation;
mod segment;
#[cfg(feature = "tls")]
mod tls;
mod types;
mod unwind;

use alloc::string::String;
pub use types::{ELFLibrary, ExternLibrary, RelocatedLibrary, Symbol};

use alloc::alloc::LayoutError;
#[cfg(not(feature = "nightly"))]
use core::convert::identity as unlikely;
#[cfg(feature = "nightly")]
use core::intrinsics::unlikely;

use elf::file::Class;

#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "aarch64",
    target_arch = "riscv64",
)))]
compile_error!("unsupport arch");

cfg_if::cfg_if! {
    if #[cfg(target_pointer_width = "64")]{
        const E_CLASS: Class = Class::ELF64;
        type Phdr = elf::segment::Elf64_Phdr;
        type Dyn = elf::dynamic::Elf64_Dyn;
        type Rela = elf::relocation::Elf64_Rela;
        type ELFSymbol = elf::symbol::Elf64_Sym;
        const REL_MASK: usize = 0xFFFFFFFF;
        const REL_BIT: usize = 32;
        const PHDR_SIZE: usize = core::mem::size_of::<elf::segment::Elf64_Phdr>();
        const EHDR_SIZE: usize = core::mem::size_of::<elf::file::Elf64_Ehdr>();
    }else{
        const E_CLASS: Class = Class::ELF32;
        type Phdr = elf::segment::Elf32_Phdr;
        type Dyn = elf::dynamic::Elf32_Dyn;
        type Rela = elf::relocation::Elf32_Rela;
        type ELFSymbol = elf::symbol::Elf32_Sym;
        const REL_MASK: usize = 0xFF;
        const REL_BIT: usize = 8;
        const PHDR_SIZE: usize = core::mem::size_of::<elf::segment::Elf32_Phdr>();
        const EHDR_SIZE: usize = core::mem::size_of::<elf::file::Elf32_Ehdr>();
    }
}

use elf::parse::ParseError;

#[derive(Debug)]
pub enum Error {
    #[cfg(feature = "std")]
    IOError {
        err: std::io::Error,
    },
    ParseError {
        err: ParseError,
    },
    #[cfg(any(feature = "libgcc", feature = "libunwind"))]
    GimliError {
        err: gimli::Error,
    },
    LoaderError {
        msg: String,
    },
    RelocateError {
        msg: String,
    },
    FindSymbolError {
        msg: String,
    },
    ValidateError {
        msg: String,
    },
    #[cfg(feature = "mmap")]
    Errno {
        err: nix::Error,
    },
    LayoutError {
        err: LayoutError,
    },
}

#[cold]
#[inline(never)]
fn parse_err_convert(err: elf::ParseError) -> Error {
    Error::ParseError { err }
}

#[cfg(any(feature = "libgcc", feature = "libunwind"))]
#[cold]
#[inline(never)]
fn gimli_err_convert(err: gimli::Error) -> Error {
    Error::GimliError { err }
}

#[cfg(feature = "std")]
#[cold]
#[inline(never)]
fn io_err_convert(err: std::io::Error) -> Error {
    Error::IOError { err }
}

#[cfg(not(feature = "mmap"))]
#[cold]
#[inline(never)]
fn layout_err_convert(err: alloc::alloc::LayoutError) -> Error {
    Error::LayoutError { err }
}

#[cfg(feature = "mmap")]
#[cold]
#[inline(never)]
fn mmap_err_convert(err: nix::Error) -> Error {
    Error::Errno { err }
}

#[cold]
#[inline(never)]
fn loader_error(msg: String) -> Error {
    Error::LoaderError { msg }
}

#[cold]
#[inline(never)]
fn relocate_error(msg: String) -> Error {
    Error::RelocateError { msg }
}

pub type Result<T> = core::result::Result<T, Error>;
