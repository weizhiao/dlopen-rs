//!A pure-rust library designed for loading ELF dynamic libraries from memory or from files.
//!
//!This library serves four purposes:
//!1. Provide a pure Rust alternative to musl ld.so or glibc ld.so.
//!2. Provide loading ELF dynamic libraries support for `#![no_std]` targets.
//!3. Easily swap out symbols in shared libraries with your own custom symbols at runtime
//!4. Faster than `ld.so` in most cases (loading dynamic libraries and getting symbols)
//!
//!Additional, it integrates seamlessly with the system’s dynamic linker in `std` environments when the `ldso` feature is enabled.
//!Currently, it supports `x86_64`, `RV64`, and `AArch64` architectures.
//!
//! # Examples
//! ```
//! use dlopen_rs::ELFLibrary;
//! use std::path::Path;
//! let path = Path::new("./target/release/libexample.so");
//!	let libc = ELFLibrary::sys_load("libc.so.6").unwrap();
//!	let libgcc = ELFLibrary::sys_load("libgcc_s.so.1").unwrap();
//! let libexample = ELFLibrary::from_file(path, None)
//!		.unwrap()
//!		.relocate(&[libgcc, libc])
//!     .finish()
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

#[cfg(feature = "debug")]
mod debug;
#[cfg(feature = "std")]
pub mod dlopen;
#[cfg(feature = "std")]
mod init;
mod loader;
mod register;
use alloc::string::{String, ToString};
use bitflags::bitflags;
use core::fmt::Display;

pub use elf_loader::Symbol;
#[cfg(feature = "std")]
pub use init::init;
pub use loader::{Dylib, ElfLibrary};
#[cfg(feature = "std")]
pub use register::dl_iterate_phdr_impl;

#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64",
)))]
compile_error!("unsupport arch");

bitflags! {
    #[derive(Clone, Copy, Debug)]
    pub struct OpenFlags:u32{
        const RTLD_LOCAL = 0;
        const RTLD_LAZY = 1;
        const RTLD_NOW= 2;
        const RTLD_NOLOAD = 4;
        const RTLD_DEEPBIND =8;
        const RTLD_GLOBAL = 256;
        const RTLD_NODELETE = 4096;
        const CUSTOM_NOT_REGISTER = 1024;
    }
}

/// dlopen-rs error type
#[derive(Debug)]
pub enum Error {
    /// Returned when encountered an io error.
    #[cfg(feature = "std")]
    IOError { err: std::io::Error },
    /// Returned when encountered a loader error.
    LoaderError { err: elf_loader::Error },
    /// Returned when failed to find a library.
    FindLibError { msg: String },
}

impl Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "std")]
            Error::IOError { err } => write!(f, "{err}"),
            Error::LoaderError { err } => write!(f, "{err}"),
            Error::FindLibError { msg } => write!(f, "{msg}"),
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

impl From<elf_loader::Error> for Error {
    #[cold]
    fn from(value: elf_loader::Error) -> Self {
        Error::LoaderError { err: value }
    }
}

#[cfg(feature = "std")]
impl From<std::io::Error> for Error {
    #[cold]
    fn from(value: std::io::Error) -> Self {
        Error::IOError { err: value }
    }
}

#[cold]
#[inline(never)]
fn find_lib_error(msg: impl ToString) -> Error {
    Error::FindLibError {
        msg: msg.to_string(),
    }
}

pub type Result<T> = core::result::Result<T, Error>;
