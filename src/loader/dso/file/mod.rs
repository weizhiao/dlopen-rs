use core::mem::MaybeUninit;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

use crate::loader::arch::EHDR_SIZE;

use super::{ehdr::ELFEhdr, Result, PHDR_SIZE};

use super::SharedObject;

#[cfg(feature = "mmap")]
mod mmap;
#[cfg(not(feature = "mmap"))]
mod no_mmap;

const BUF_SIZE: usize = EHDR_SIZE + 11 * PHDR_SIZE;

pub(crate) struct ELFFile {
    file: File,
}

impl ELFFile {
    pub(crate) fn new<P: AsRef<std::ffi::OsStr>>(path: P) -> Result<ELFFile> {
        let file = File::open(path.as_ref())?;
        Ok(ELFFile { file })
    }
}

impl SharedObject for ELFFile {
    fn parse_ehdr(&mut self) -> Result<Vec<u8>> {
        let mut buf: MaybeUninit<[u8; BUF_SIZE]> = MaybeUninit::uninit();
        self.file.read_exact(unsafe { &mut *buf.as_mut_ptr() })?;
        let buf = unsafe { buf.assume_init() };
        let ehdr = ELFEhdr::new(&buf)?;
        ehdr.validate()?;

        //获取phdrs
        let (phdr_start, phdr_end) = ehdr.phdr_range();
        let phdrs_size = phdr_end - phdr_start;
        let mut phdrs = Vec::with_capacity(phdrs_size);
        if phdr_end > BUF_SIZE {
            unsafe { phdrs.set_len(phdrs_size) };
            self.file.seek(SeekFrom::Start(phdr_start as _))?;
            self.file.read_exact(&mut phdrs)?;
        } else {
            phdrs.extend_from_slice(&buf[phdr_start..phdr_end]);
        };
        Ok(phdrs)
    }
}
