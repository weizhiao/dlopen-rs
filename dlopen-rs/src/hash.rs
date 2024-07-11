use crate::{unlikely, Symbol};
use elf::string_table::StringTable;

#[derive(Debug)]
pub(crate) enum ELFHashTable {
    Gnu(ELFGnuHash),
}

impl ELFHashTable {
    pub(crate) unsafe fn find(
        &self,
        name: &[u8],
        symtab: *const Symbol,
        strtab: &StringTable<'static>,
    ) -> Option<Symbol> {
        match self {
            ELFHashTable::Gnu(hash_table) => hash_table.find(name, symtab, strtab),
        }
    }

    pub(crate) fn parse_gnu_hash(ptr: *const u8) -> ELFHashTable {
        ELFHashTable::Gnu(unsafe { ELFGnuHash::parse(ptr) })
    }
}

#[derive(Debug)]
struct ELFGnuHash {
    //不使用bloom，因此也就不保存bloom的信息
    nbucket: u32,
    table_start_idx: u32,
    buckets: *const u32,
    chains: *const u32,
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
        reader.add(4);
        let bloom_size = nbloom as usize * core::mem::size_of::<usize>();
        reader.add(bloom_size);
        let buckets = reader.as_ptr() as _;
        reader.add(nbucket as usize * core::mem::size_of::<u32>());
        let chains = reader.as_ptr() as _;
        ELFGnuHash {
            nbucket,
            table_start_idx,
            buckets,
            chains,
        }
    }

    #[inline]
    fn gnu_hash(name: &[u8]) -> u32 {
        let mut hash = 5381u32;
        for byte in name {
            hash = hash.wrapping_mul(33).wrapping_add(u32::from(*byte));
        }
        hash
    }

    pub(crate) unsafe fn find(
        &self,
        name: &[u8],
        symtab: *const Symbol,
        strtab: &StringTable<'static>,
    ) -> Option<Symbol> {
        let hash = ELFGnuHash::gnu_hash(name);
        let table_start_idx = self.table_start_idx as usize;
        let chain_start_idx = self
            .buckets
            .add((hash as usize) % self.nbucket as usize)
            .read() as usize;
        if unlikely(chain_start_idx == 0) {
            return None;
        }

        let mut cur_chain = self.chains.add(chain_start_idx - table_start_idx);
        let mut cur_symbol_ptr = symtab.add(chain_start_idx);
        loop {
            let chain_hash = cur_chain.read();
            if hash | 1 == chain_hash | 1 {
                let cur_symbol = cur_symbol_ptr.read();
                let sym_name = strtab.get_raw(cur_symbol.st_name as usize).unwrap();
                if sym_name == name {
                    return Some(cur_symbol);
                }
            }
            if unlikely(chain_hash & 1 != 0) {
                break;
            }
            cur_chain = cur_chain.add(1);
            cur_symbol_ptr = cur_symbol_ptr.add(1);
        }
        None
    }
}
