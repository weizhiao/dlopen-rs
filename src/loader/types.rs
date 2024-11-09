use crate::{
    loader::arch::{EHDR_SIZE, EM_ARCH, E_CLASS},
    parse_ehdr_error, Result,
};
use elf::{
    abi::*,
    endian::NativeEndian,
    file::{parse_ident, FileHeader},
};
pub(crate) struct ELFEhdr {
    ehdr: FileHeader<NativeEndian>,
}

impl ELFEhdr {
    pub(crate) fn new(data: &[u8]) -> Result<ELFEhdr> {
        let ident_buf = &data[..EI_NIDENT];
        let tail_buf = &data[EI_NIDENT..EHDR_SIZE];
        let ident = parse_ident::<NativeEndian>(&ident_buf).map_err(parse_ehdr_error)?;
        let ehdr = FileHeader::parse_tail(ident, &tail_buf).map_err(parse_ehdr_error)?;
        Ok(ELFEhdr { ehdr })
    }

    //验证elf头
    #[inline]
    pub(crate) fn validate(&self) -> Result<()> {
        if self.ehdr.e_type != ET_DYN {
            return Err(parse_ehdr_error("file type mismatch"));
        }

        if self.ehdr.e_machine != EM_ARCH {
            return Err(parse_ehdr_error("file arch mismatch"));
        }

        if self.ehdr.class != E_CLASS {
            return Err(parse_ehdr_error("file class mismatch"));
        }

        Ok(())
    }

    pub(crate) fn e_phnum(&self) -> usize {
        self.ehdr.e_phnum as usize
    }

    pub(crate) fn e_phentsize(&self) -> usize {
        self.ehdr.e_phentsize as usize
    }

    pub(crate) fn e_phoff(&self) -> usize {
        self.ehdr.e_phoff as usize
    }

    pub(crate) fn phdr_range(&self) -> (usize, usize) {
        let phdrs_size = self.e_phentsize() * self.e_phnum();
        let phdr_start = self.e_phoff();
        let phdr_end = phdr_start + phdrs_size;
        (phdr_start, phdr_end)
    }
}
