#[derive(Debug, Clone)]
pub(crate) struct ELFGnuHash {
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
