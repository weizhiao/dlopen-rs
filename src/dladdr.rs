use crate::{register::MANAGER, Dylib, ElfLibrary};
use core::{ffi::CStr, fmt::Debug};

pub struct DlInfo {
    /// dylib
    dylib: Dylib,
    /// Name of symbol whose definition overlaps addr
    sname: Option<&'static CStr>,
    /// Exact address of symbol named in dli_sname
    saddr: usize,
}

impl DlInfo {
    #[inline]
    pub fn dylib(&self) -> &Dylib {
        &self.dylib
    }

    /// Name of symbol whose definition overlaps addr
    #[inline]
    pub fn symbol_name(&self) -> Option<&CStr> {
        self.sname
    }

    /// Exact address of symbol
    #[inline]
    pub fn symbol_addr(&self) -> Option<usize> {
        if self.saddr == 0 {
            None
        } else {
            Some(self.saddr)
        }
    }
}

impl Debug for DlInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DlInfo")
            .field("dylib", &self.dylib)
            .field("sname", &self.sname)
            .field("saddr", &format_args!("{:#x}", self.saddr))
            .finish()
    }
}

impl ElfLibrary {
    fn addr2dso(addr: usize) -> Option<Dylib> {
        MANAGER.read().all.values().find_map(|v| {
            let start = v.relocated_dylib_ref().base();
            let end = start + v.relocated_dylib_ref().map_len();
            if (start..end).contains(&addr) {
                Some(v.get_dylib())
            } else {
                None
            }
        })
    }

    /// determines whether the address specified in addr is located in one of the shared objects loaded by the calling
    /// application.  If it is, then `dladdr` returns information about the shared object and
    /// symbol that overlaps addr.
    pub fn dladdr(addr: usize) -> Option<DlInfo> {
        log::info!(
            "dladdr: Try to find the symbol information corresponding to [{:#x}]",
            addr
        );
        Self::addr2dso(addr).map(|dylib| {
            let mut dl_info = DlInfo {
                dylib,
                sname: None,
                saddr: 0,
            };
            let symtab = dl_info.dylib.inner.symtab();
            for i in 0..symtab.count_syms() {
                let (sym, syminfo) = symtab.symbol_idx(i);
                let start = dl_info.dylib.base() + sym.st_value();
                let end = start + sym.st_size();
                if sym.st_value() != 0
                    && sym.is_ok_bind()
                    && sym.is_ok_type()
                    && (start..end).contains(&addr)
                {
                    dl_info.sname = Some(unsafe { core::mem::transmute(syminfo.cname().unwrap()) });
                    dl_info.saddr = start;
                }
            }
            dl_info
        })
    }
}
