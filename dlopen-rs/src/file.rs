use crate::{ehdr::ELFEhdr, Phdr, Result, EHDR_SIZE, PHDR_SIZE};

pub(crate) enum FileType {
    #[cfg(feature = "std")]
    Fd(std::fs::File),
    Binary(&'static [u8]),
}

pub(crate) struct ELFFile {
    pub(crate) context: FileType,
}

#[cfg(feature = "std")]
const BUF_SIZE: usize = EHDR_SIZE + 11 * PHDR_SIZE;

#[cfg(feature = "std")]
pub(crate) struct Buf {
    stack: core::mem::MaybeUninit<[u8; BUF_SIZE]>,
    heap: Vec<u8>,
}

#[cfg(feature = "std")]
impl Buf {
    pub(crate) fn new() -> Buf {
        Buf {
            stack: core::mem::MaybeUninit::uninit(),
            heap: Vec::new(),
        }
    }

    fn stack(&mut self) -> &mut [u8] {
        unsafe { &mut *self.stack.as_mut_ptr() }
    }

    fn stack_ptr(&mut self) -> *mut u8 {
        self.stack.as_mut_ptr() as _
    }

    fn heap(&mut self) -> &mut Vec<u8> {
        &mut self.heap
    }

    fn heap_ptr(&mut self) -> *mut u8 {
        self.heap.as_mut_ptr()
    }
}

#[cfg(not(feature = "std"))]
pub(crate) struct Buf;

impl ELFFile {
    #[cfg(feature = "std")]
    #[inline]
    pub(crate) fn from_file<P: AsRef<std::ffi::OsStr>>(path: P) -> Result<ELFFile> {
        use crate::io_err_convert;
        use std::fs::File;
        let file = File::open(path.as_ref()).map_err(io_err_convert)?;
        Ok(ELFFile {
            context: FileType::Fd(file),
        })
    }

    #[inline]
    pub(crate) fn from_binary(bytes: &[u8]) -> ELFFile {
        ELFFile {
            context: FileType::Binary(unsafe { core::mem::transmute(bytes) }),
        }
    }

    // result lifetime is same to buf
    pub(crate) fn parse_phdrs<'a, 'b>(&'a mut self, buf: &'b mut Buf) -> Result<&'b [Phdr]> {
        let phdrs = match &mut self.context {
            #[cfg(feature = "std")]
            FileType::Fd(file) => {
                use crate::io_err_convert;
                use std::{
                    io::Seek,
                    io::{Read, SeekFrom},
                };

                let stack_buf = buf.stack();
                file.read_exact(stack_buf).map_err(io_err_convert)?;
                let ehdr = ELFEhdr::new(&stack_buf)?;
                ehdr.validate()?;

                //获取phdrs
                let phdrs_num = ehdr.e_phnum();
                let (phdr_start, phdr_end) = ehdr.phdr_range();
                let phdrs_size = phdr_end - phdr_start;
                let phdrs = if phdr_end > BUF_SIZE {
                    let heap = buf.heap();
                    heap.reserve(phdrs_size);
                    unsafe { heap.set_len(phdrs_size) };
                    file.seek(SeekFrom::Start(phdr_start as _))
                        .map_err(io_err_convert)?;
                    file.read_exact(heap).map_err(io_err_convert)?;
                    unsafe { core::slice::from_raw_parts(buf.heap_ptr() as _, phdrs_num) }
                } else {
                    unsafe {
                        let ptr = buf.stack_ptr();
                        core::slice::from_raw_parts(ptr.add(phdr_start) as _, phdrs_num)
                    }
                };
                phdrs
            }
            FileType::Binary(file) => {
                let ehdr = ELFEhdr::new(*file)?;
                ehdr.validate()?;

                let phdrs_num = ehdr.e_phnum();
                let (phdr_start, _) = ehdr.phdr_range();
                let phdrs = unsafe {
                    core::slice::from_raw_parts(file.as_ptr().add(phdr_start) as _, phdrs_num)
                };
                phdrs
            }
        };
        Ok(phdrs)
    }
}
