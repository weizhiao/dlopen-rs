//!A versatile Rust library designed for loading ELF dynamic libraries from memory or from files.
//!
//!This library serves three purposes:
//!1. Provide a pure Rust alternative to musl ld.so or glibc ld.so.
//!2. Provide loading ELF dynamic libraries support for `#![no_std]` targets.
//!3. Easily swap out symbols in shared libraries with your own custom symbols at runtime
//!
//!Additional, it integrates seamlessly with the system’s dynamic linker in `std` environments when the `ldso` feature is enabled.
//!Currently, it supports `x86_64`, `x86`, `RV64`, and `AArch64` architectures.
//!
//! # Examples
//! ```
//! use dlopen_rs::ELFLibrary;
//! use std::path::Path;
//! let path = Path::new("./target/release/libexample.so");
//!	let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
//!	let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();
//! let libexample = ELFLibrary::from_file(path)
//!		.unwrap()
//!		.relocate(&[libgcc, libc])
//!		.unwrap();
//!
//! let add = unsafe {
//! 	libexample
//! 	.get::<fn(i32, i32) -> i32>("add")
//! 	.unwrap()
//! };
//! println!("{}", add(1,1));
//! ```
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

mod builtin;
#[cfg(feature = "debug")]
mod debug;
#[cfg(feature = "std")]
mod dlopen;
#[cfg(feature = "ldso")]
mod ldso;
mod loader;
#[cfg(feature = "std")]
mod register;
mod types;

use alloc::string::{String, ToString};
use core::fmt::Display;
pub use loader::{
    mmap::{MapFlags, Mmap, MmapImpl, Offset, OffsetType, ProtFlags},
    ELFLibrary, PAGE_SIZE,
};
pub use types::{RelocatedLibrary, Symbol};

#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "x86",
    target_arch = "aarch64",
    target_arch = "riscv64",
)))]
compile_error!("unsupport arch");

#[derive(Debug)]
pub enum Error {
    /// Returned when encountered an io error.
    #[cfg(feature = "std")]
    IOError {
        err: std::io::Error,
    },
    MmapError {
        msg: String,
    },
    #[cfg(any(feature = "libgcc", feature = "libunwind"))]
    GimliError {
        err: gimli::Error,
    },
    #[cfg(feature = "ldso")]
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
            Error::MmapError { msg } => write!(f, "{msg}"),
            #[cfg(any(feature = "libgcc", feature = "libunwind"))]
            Error::GimliError { err } => write!(f, "{err}"),
            #[cfg(feature = "ldso")]
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
            _ => None,
        }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    #[cold]
    fn from(value: std::io::Error) -> Self {
        Error::IOError { err: value }
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

#[cfg(feature = "ldso")]
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
