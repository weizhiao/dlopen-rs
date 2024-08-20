use crate::Result;
use core::ffi::CStr;

#[derive(Debug)]
pub(crate) struct ELFStringTable<'data> {
    data: &'data [u8],
}

impl<'data> ELFStringTable<'data> {
    pub(crate) fn new(data: &'data [u8]) -> Self {
        ELFStringTable { data }
    }

    pub(crate) fn get_cstr(&self, offset: usize) -> Result<&'data CStr> {
        let start = self.data.get(offset..).unwrap();
        Ok(unsafe { CStr::from_ptr(start.as_ptr() as _) })
    }

    pub(crate) fn get_str(&self, offset: usize) -> Result<&'data str> {
        let start = self.data.get(offset..).unwrap();
        let end = start.iter().position(|&b| b == 0u8).unwrap();
        Ok(core::str::from_utf8(start.split_at(end).0).unwrap())
    }
}
