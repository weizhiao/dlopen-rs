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
//! let f = unsafe {
//! 	libexample
//! 	.get::<extern "C" fn(i32) -> i32>("c_fun_add_two")
//! 	.unwrap()
//! };
//! println!("{}", f(2));
//! let f = unsafe {
//! 	libexample
//! 	.get::<extern "C" fn()>("c_fun_print_something_else")
//! 	.unwrap()
//! };
//! f();
//! ```
#![cfg_attr(feature = "nightly", allow(internal_features))]
#![cfg_attr(feature = "nightly", feature(core_intrinsics))]
#![cfg_attr(all(feature = "nightly", not(feature = "std")), feature(error_in_core))]
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

mod arch;
mod builtin;
mod dso;
mod dynamic;
mod ehdr;
mod hashtable;
#[cfg(feature = "load_self")]
mod load_self;
mod relocation;
mod segment;
#[cfg(feature = "tls")]
mod tls;
mod types;
mod unwind;

use alloc::string::{String, ToString};
pub use types::{ELFLibrary, ExternLibrary, RelocatedLibrary, Symbol};

#[cfg(not(feature = "nightly"))]
use core::convert::identity as unlikely;
use core::fmt::Display;
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

#[derive(Debug)]
pub enum Error {
    #[cfg(feature = "std")]
    IOError {
        err: std::io::Error,
    },
    #[cfg(feature = "mmap")]
    MmapError {
        err: nix::Error,
    },
    #[cfg(any(feature = "libgcc", feature = "libunwind"))]
    GimliError {
        err: gimli::Error,
    },
    #[cfg(feature = "load_self")]
    FindLibError {
        msg: String,
    },
    #[cfg(feature = "tls")]
    TLSError {
        msg: &'static str,
    },
    RelocateError {
        msg: String,
    },
    FindSymbolError {
        msg: String,
    },
    ParseDynamicError {
        msg: &'static str,
    },
    ParseEhdrError {
        msg: String,
    },
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "std")]
            Error::IOError { err } => write!(f, "{err}"),
            #[cfg(feature = "mmap")]
            Error::MmapError { err } => write!(f, "{err}"),
            #[cfg(any(feature = "libgcc", feature = "libunwind"))]
            Error::GimliError { err } => write!(f, "{err}"),
            #[cfg(feature = "load_self")]
            Error::FindLibError { msg } => write!(f, "{msg}"),
            #[cfg(feature = "tls")]
            Error::TLSError { msg } => write!(f, "{msg}"),
            Error::RelocateError { msg } => write!(f, "{msg}"),
            Error::FindSymbolError { msg } => write!(f, "{msg}"),
            Error::ParseDynamicError { msg } => write!(f, "{msg}"),
            Error::ParseEhdrError { msg } => write!(f, "{msg}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::IOError { err } => Some(err),
			#[cfg(feature = "mmap")]
            Error::MmapError { err } => Some(err),
            _ => None,
        }
    }
}

#[cfg(all(feature = "nightly", not(feature = "std")))]
impl core::error::Error for Error {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        None
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    #[cold]
    fn from(value: std::io::Error) -> Self {
        Error::IOError { err: value }
    }
}

#[cfg(feature = "mmap")]
impl From<nix::Error> for Error {
    #[cold]
    fn from(value: nix::Error) -> Self {
        Error::MmapError { err: value }
    }
}

#[cfg(any(feature = "libgcc", feature = "libunwind"))]
impl From<gimli::Error> for Error {
	#[cold]
    fn from(value: gimli::Error) -> Self {
        Error::GimliError { err: value }
    }
}

#[cfg(feature = "tls")]
#[cold]
#[inline(never)]
fn tls_error(msg: &'static str) -> Error {
    Error::TLSError { msg }
}

#[cfg(feature = "load_self")]
#[cold]
#[inline(never)]
fn find_lib_error(msg: impl ToString) -> Error {
    Error::FindLibError {
        msg: msg.to_string(),
    }
}

#[cold]
#[inline(never)]
fn relocate_error(msg: impl ToString) -> Error {
    Error::RelocateError {
        msg: msg.to_string(),
    }
}

#[cold]
#[inline(never)]
fn find_symbol_error(msg: impl ToString) -> Error {
    Error::FindSymbolError {
        msg: msg.to_string(),
    }
}

#[cold]
#[inline(never)]
fn parse_dynamic_error(msg: &'static str) -> Error {
    Error::ParseDynamicError { msg }
}

#[cold]
#[inline(never)]
fn parse_ehdr_error(msg: impl ToString) -> Error {
    Error::ParseEhdrError {
        msg: msg.to_string(),
    }
}

pub type Result<T> = core::result::Result<T, Error>;
