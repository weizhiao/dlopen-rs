use crate::loader::arch::ELFSymbol;
use core::ops::Shr;

#[derive(Clone)]
struct ELFGnuHash {
    pub nbucket: u32,
    pub table_start_idx: u32,
    pub nshift: u32,
    pub blooms: &'static [usize],
    pub buckets: *const u32,
    pub chains: *const u32,
}

impl ELFGnuHash {
    #[inline]
    pub(crate) unsafe fn parse(ptr: *const u8) -> ELFGnuHash {
        struct Reader {
            ptr: *const u8,
        }

        impl Reader {
            #[inline]
            fn new(ptr: *const u8) -> Reader {
                Reader { ptr }
            }

            #[inline]
            unsafe fn read<T>(&mut self) -> T {
                let value = self.ptr.cast::<T>().read();
                self.ptr = self.ptr.add(core::mem::size_of::<T>());
                value
            }

            #[inline]
            //字节为单位
            unsafe fn add(&mut self, count: usize) {
                self.ptr = self.ptr.add(count);
            }

            #[inline]
            fn as_ptr(&self) -> *const u8 {
                self.ptr
            }
        }

        let mut reader = Reader::new(ptr);

        let nbucket: u32 = reader.read();
        let table_start_idx: u32 = reader.read();
        let nbloom: u32 = reader.read();
        let nshift: u32 = reader.read();
        let blooms_ptr = reader.as_ptr() as *const usize;
        let blooms = core::slice::from_raw_parts(blooms_ptr, nbloom as _);
        let bloom_size = nbloom as usize * core::mem::size_of::<usize>();
        reader.add(bloom_size);
        let buckets = reader.as_ptr() as _;
        reader.add(nbucket as usize * core::mem::size_of::<u32>());
        let chains = reader.as_ptr() as _;
        ELFGnuHash {
            nbucket,
            blooms,
            nshift,
            table_start_idx,
            buckets,
            chains,
        }
    }

    #[inline]
    pub(crate) fn gnu_hash(name: &[u8]) -> u32 {
        let mut hash = 5381u32;
        for byte in name {
            hash = hash.wrapping_mul(33).wrapping_add(u32::from(*byte));
        }
        hash
    }
}

#[derive(Clone)]
pub(crate) struct ELFStringTable {
    data: &'static [u8],
}

impl ELFStringTable {
    fn new(data: &'static [u8]) -> Self {
        ELFStringTable { data }
    }

    pub(crate) fn get(&self, offset: usize) -> &'static str {
        let start = self.data.get(offset..).unwrap();
        let end = start.iter().position(|&b| b == 0u8).unwrap();
        unsafe { core::str::from_utf8_unchecked(start.split_at(end).0) }
    }
}

pub(crate) struct SymbolData {
    /// .gnu.hash
    hashtab: ELFGnuHash,
    /// .dynsym
    symtab: *const ELFSymbol,
    /// .dynstr
    strtab: ELFStringTable,
    #[cfg(feature = "version")]
    /// .gnu.version
    pub version: Option<super::version::ELFVersion>,
}

pub(crate) struct SymbolInfo<'a> {
    pub name: &'a str,
    #[cfg(feature = "version")]
    version: Option<super::version::SymbolVersion<'a>>,
}

impl<'a> SymbolInfo<'a> {
    pub(crate) const fn new(name: &'a str) -> Self {
        SymbolInfo {
            name,
            #[cfg(feature = "version")]
            version: None,
        }
    }

    #[cfg(feature = "version")]
    pub(crate) const fn new_with_version(
        name: &'a str,
        version: super::version::SymbolVersion<'a>,
    ) -> Self {
        SymbolInfo {
            name,
            version: Some(version),
        }
    }
}

impl SymbolData {
    #[cfg(not(feature = "version"))]
    pub(crate) fn new(
        hash_off: usize,
        symtab_off: usize,
        strtab_off: usize,
        strtab_size: usize,
    ) -> Self {
        let hashtab = unsafe { ELFGnuHash::parse(hash_off as *const u8) };
        let symtab = symtab_off as *const ELFSymbol;
        let strtab = ELFStringTable::new(unsafe {
            core::slice::from_raw_parts(strtab_off as *const u8, strtab_size)
        });
        SymbolData {
            hashtab,
            symtab,
            strtab,
        }
    }

    #[cfg(feature = "version")]
    pub(crate) fn new(
        hash_off: usize,
        symtab_off: usize,
        strtab_off: usize,
        strtab_size: usize,
        version_ids_off: Option<usize>,
        verneeds: Option<(usize, usize)>,
        verdefs: Option<(usize, usize)>,
    ) -> Self {
        use super::version::ELFVersion;
        let hashtab = unsafe { ELFGnuHash::parse(hash_off as *const u8) };
        let symtab = symtab_off as *const ELFSymbol;
        let strtab = ELFStringTable::new(unsafe {
            core::slice::from_raw_parts(strtab_off as *const u8, strtab_size)
        });
        let version = ELFVersion::new(version_ids_off, verneeds, verdefs, &strtab);
        SymbolData {
            hashtab,
            symtab,
            strtab,
            version,
        }
    }

    pub(crate) fn strtab(&self) -> &ELFStringTable {
        &self.strtab
    }

    pub(crate) fn get_sym(&self, symbol: &SymbolInfo) -> Option<&ELFSymbol> {
        let hash = ELFGnuHash::gnu_hash(symbol.name.as_bytes());
        let bloom_width: u32 = 8 * size_of::<usize>() as u32;
        let bloom_idx = (hash / (bloom_width)) as usize % self.hashtab.blooms.len();
        let filter = self.hashtab.blooms[bloom_idx] as u64;
        if filter & (1 << (hash % bloom_width)) == 0 {
            return None;
        }
        let hash2 = hash.shr(self.hashtab.nshift);
        if filter & (1 << (hash2 % bloom_width)) == 0 {
            return None;
        }
        let table_start_idx = self.hashtab.table_start_idx as usize;
        let chain_start_idx = unsafe {
            self.hashtab
                .buckets
                .add((hash as usize) % self.hashtab.nbucket as usize)
                .read()
        } as usize;
        if chain_start_idx == 0 {
            return None;
        }
        let mut dynsym_idx = chain_start_idx;
        let mut cur_chain = unsafe { self.hashtab.chains.add(dynsym_idx - table_start_idx) };
        let mut cur_symbol_ptr = unsafe { self.symtab.add(dynsym_idx) };
        loop {
            let chain_hash = unsafe { cur_chain.read() };
            if hash | 1 == chain_hash | 1 {
                let cur_symbol = unsafe { &*cur_symbol_ptr };
                let sym_name = self.strtab.get(cur_symbol.st_name as usize);
                #[cfg(feature = "version")]
                if sym_name == symbol.name && self.check_match(dynsym_idx, &symbol.version) {
                    return Some(cur_symbol);
                }
                #[cfg(not(feature = "version"))]
                if sym_name == symbol.name {
                    return Some(cur_symbol);
                }
            }
            if chain_hash & 1 != 0 {
                break;
            }
            cur_chain = unsafe { cur_chain.add(1) };
            cur_symbol_ptr = unsafe { cur_symbol_ptr.add(1) };
            dynsym_idx += 1;
        }
        None
    }

    pub(crate) fn rel_symbol(&self, idx: usize) -> (&ELFSymbol, SymbolInfo) {
        let symbol = unsafe { &*self.symtab.add(idx) };
        let name = self.strtab.get(symbol.st_name as usize);
        (
            symbol,
            SymbolInfo {
                name,
                #[cfg(feature = "version")]
                version: self.get_requirement(idx),
            },
        )
    }
}
