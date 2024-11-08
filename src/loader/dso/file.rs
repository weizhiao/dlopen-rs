use super::mmap::OffsetType;
use super::{
    mmap::{self, RawData},
    types::ELFEhdr,
    MapSegment, Result, PHDR_SIZE,
};
use super::{SharedObject, MASK};
use crate::loader::arch::EHDR_SIZE;
use core::{mem::MaybeUninit, ops::Range};
use std::os::fd::AsRawFd;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

const BUF_SIZE: usize = EHDR_SIZE + 11 * PHDR_SIZE;

pub(crate) struct ELFFile {
    file: File,
}

impl ELFFile {
    pub(crate) fn new(file: File) -> Self {
        ELFFile { file }
    }
}

impl RawData for ELFFile {
    fn transport(&self, offset: usize, len: usize) -> mmap::Offset {
        mmap::Offset {
            offset: offset - (offset & MASK),
            len,
            kind: OffsetType::File {
                fd: self.file.as_raw_fd(),
                file_offset: offset,
            },
        }
    }
}

impl MapSegment for ELFFile {}

impl SharedObject for ELFFile {
    fn parse_ehdr(&mut self) -> Result<(Range<usize>, Vec<u8>)> {
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
        Ok((phdr_start..phdr_end, phdrs))
    }
}
