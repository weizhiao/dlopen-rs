use crate::{parse_err_convert, unlikely, Error, Result, EHDR_SIZE, E_CLASS};
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
        let ident = parse_ident::<NativeEndian>(&ident_buf).map_err(parse_err_convert)?;
        let ehdr = FileHeader::parse_tail(ident, &tail_buf).map_err(parse_err_convert)?;
        Ok(ELFEhdr { ehdr })
    }

    //验证elf头
    #[inline]
    pub(crate) fn validate(&self) -> Result<()> {
        #[cfg(target_arch = "x86_64")]
        const EM_ARCH: u16 = EM_X86_64;
        #[cfg(target_arch = "x86")]
        const EM_ARCH: u16 = EM_386;
        #[cfg(target_arch = "aarch64")]
        const EM_ARCH: u16 = EM_AARCH64;
        #[cfg(target_arch = "arm")]
        const EM_ARCH: u16 = EM_ARM;
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        const EM_ARCH: u16 = EM_RISCV;

        if unlikely(self.ehdr.e_type != ET_DYN) {
            return Err(Error::FileTypeMismatch);
        }

        if unlikely(self.ehdr.e_machine != EM_ARCH) {
            return Err(Error::ArchMismatch);
        }

        if unlikely(self.ehdr.class != E_CLASS) {
            return Err(Error::ClassMismatch);
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
