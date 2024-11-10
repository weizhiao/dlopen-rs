use super::{ELFLibrary, ELFSegments, EhFrame, Rela, UserData, REL_BIT};
use crate::{
    loader::arch::{EHDR_SIZE, EM_ARCH, E_CLASS},
    parse_ehdr_error, RelocatedLibrary, Result,
};
use alloc::{fmt::Debug, format, string::String, vec::Vec};
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

#[derive(Default)]
pub(crate) struct ELFRelocation {
    pub(crate) pltrel: Option<ELFRelaArray>,
    pub(crate) dynrel: Option<ELFRelaArray>,
}

#[derive(PartialEq, Eq)]
enum RelocateStage {
    Init,
    Relocating(bool),
    Finish,
}

pub(crate) struct RelocateState {
    // 位图用于记录对应的项是否已经被重定位，已经重定位的项对应的bit会设为1
    relocated: BitMap,
    stage: RelocateStage,
}

pub(crate) struct ELFRelaArray {
    array: &'static [Rela],
    state: RelocateState,
}

struct BitMapIterator<'bitmap> {
    cur_bit: u32,
    index: usize,
    state: &'bitmap mut RelocateState,
}

impl<'bitmap> BitMapIterator<'bitmap> {
    fn new(state: &'bitmap mut RelocateState) -> Self {
        Self {
            cur_bit: state.relocated.unit(0),
            index: 0,
            state,
        }
    }

    fn next(&mut self) -> Option<(&mut RelocateState, usize)> {
        loop {
            let idx = self.cur_bit.trailing_ones();
            if idx == 32 {
                self.index += 1;
                if self.index == self.state.relocated.unit_count() {
                    break None;
                }
                self.cur_bit = self.state.relocated.unit(self.index);
            } else {
                self.cur_bit |= 1 << idx;
                break Some((self.state, self.index * 32 + idx as usize));
            }
        }
    }
}

impl ELFRelaArray {
    fn not_relocated(&mut self, lib: &ELFLibrary) -> String {
        let mut f = String::new();
        let mut iter = BitMapIterator::new(&mut self.state);
        while let Some((_, idx)) = iter.next() {
            let rela = &self.array[idx];
            let r_sym = rela.r_info as usize >> REL_BIT;
            let (_, syminfo) = lib.symbols().rel_symbol(r_sym);
            f.push_str(&format!("[{}] ", syminfo.name));
        }
        f
    }

    pub(crate) fn is_finished(&self) -> bool {
        let mut finished = true;
        if self.state.stage != RelocateStage::Finish {
            for unit in &self.state.relocated.bitmap {
                if unit.count_zeros() != 0 {
                    finished = false;
                    break;
                }
            }
        }
        finished
    }

    pub(crate) fn relocate(
        &mut self,
        f: impl Fn(&Rela, usize, &mut RelocateState, fn(usize, &mut RelocateState)),
    ) {
        match self.state.stage {
            RelocateStage::Init => {
                let deal_fail = |idx: usize, state: &mut RelocateState| {
                    state.relocated.clear(idx);
                    state.stage = RelocateStage::Relocating(false);
                };
                for (idx, rela) in self.array.iter().enumerate() {
                    f(rela, idx, &mut self.state, deal_fail);
                }
                if self.state.stage == RelocateStage::Init {
                    self.state.stage = RelocateStage::Finish;
                }
            }
            RelocateStage::Relocating(_) => {
                let deal_fail = |_idx: usize, state: &mut RelocateState| {
                    // 重定位失败，设置标识位
                    state.stage = RelocateStage::Relocating(false);
                };
                let mut iter = BitMapIterator::new(&mut self.state);
                while let Some((state, idx)) = iter.next() {
                    state.stage = RelocateStage::Relocating(true);
                    f(&self.array[idx], idx, state, deal_fail);
                    if state.stage == RelocateStage::Relocating(true) {
                        state.relocated.set(idx);
                    }
                }
            }
            RelocateStage::Finish => {}
        }
    }
}

impl ELFRelocation {
    pub(crate) fn new(pltrel: Option<&'static [Rela]>, dynrel: Option<&'static [Rela]>) -> Self {
        let pltrel = pltrel.map(|array| ELFRelaArray {
            array,
            state: RelocateState {
                relocated: BitMap::new(array.len()),
                stage: RelocateStage::Init,
            },
        });
        let dynrel = dynrel.map(|array| ELFRelaArray {
            array,
            state: RelocateState {
                relocated: BitMap::new(array.len()),
                stage: RelocateStage::Init,
            },
        });
        Self { pltrel, dynrel }
    }

    pub(crate) fn is_finished(&self) -> bool {
        let mut finished = true;
        if let Some(array) = &self.pltrel {
            finished = array.is_finished();
        }
        if let Some(array) = &self.dynrel {
            finished = array.is_finished();
        }
        finished
    }

    #[cold]
    pub(crate) fn not_relocated(&mut self, lib: &ELFLibrary) -> String {
        let mut f = String::new();
        f.push_str(&format!(
            "{}: The symbols that have not been relocated:   ",
            lib.name()
        ));
        if let Some(array) = &mut self.pltrel {
            f.push_str(array.not_relocated(lib).as_str());
        }
        if let Some(array) = &mut self.dynrel {
            f.push_str(array.not_relocated(lib).as_str());
        }
        f
    }
}

pub(crate) struct BitMap {
    bitmap: Vec<u32>,
}

impl BitMap {
    fn new(size: usize) -> Self {
        let bitmap_size = (size + 31) / 32;
        let mut bitmap = Vec::with_capacity(bitmap_size);
        // 初始时全部标记为已重定位
        bitmap.resize(bitmap_size, u32::MAX);
        Self { bitmap }
    }

    fn unit(&self, index: usize) -> u32 {
        self.bitmap[index]
    }

    fn unit_count(&self) -> usize {
        self.bitmap.len()
    }

    fn set(&mut self, bit_index: usize) {
        let unit_index = bit_index / 32;
        let index = bit_index % 32;
        self.bitmap[unit_index] |= 1 << index;
    }

    fn clear(&mut self, bit_index: usize) {
        let unit_index = bit_index / 32;
        let index = bit_index % 32;
        self.bitmap[unit_index] &= !(1 << index);
    }
}

#[allow(unused)]
pub(crate) struct ExtraData {
    /// phdrs
    #[cfg(feature = "std")]
    pub phdrs: Option<&'static [super::Phdr]>,
    /// .eh_frame
    pub unwind: Option<EhFrame>,
    /// semgents
    pub segments: ELFSegments,
    /// .fini
    pub fini_fn: Option<extern "C" fn()>,
    /// .fini_array
    pub fini_array_fn: Option<&'static [extern "C" fn()]>,
    /// .tbss and .tdata
    #[cfg(feature = "tls")]
    pub tls: Option<Box<super::tls::ELFTLS>>,
    /// user data
    pub user_data: UserData,
    /// dependency libraries
    pub dep_libs: Vec<RelocatedLibrary>,
}

impl Debug for ExtraData {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut binding = f.debug_struct("ExtraData");
        let f = binding.field("segments", &self.segments);
        f.field("dep_libs", &self.dep_libs);
        f.finish()
    }
}

impl ExtraData {
    #[cfg(feature = "std")]
    pub(crate) fn phdrs(&self) -> Option<&[super::Phdr]> {
        self.phdrs
    }

    #[inline]
    pub(crate) fn base(&self) -> usize {
        self.segments.base()
    }

    #[inline]
    #[cfg(feature = "tls")]
    pub(crate) fn tls(&self) -> Option<*const super::tls::ELFTLS> {
        self.tls.as_ref().map(|val| val.as_ref() as _)
    }

    #[inline]
    pub(crate) fn user_data(&mut self) -> &mut UserData {
        &mut self.user_data
    }

    #[inline]
    pub(crate) fn insert_dep_libs(&mut self, dep_libs: impl AsRef<[RelocatedLibrary]>) {
        self.dep_libs.extend_from_slice(dep_libs.as_ref());
    }

    #[inline]
    pub(crate) fn get_dep_libs(&self) -> &Vec<RelocatedLibrary> {
        &self.dep_libs
    }

    pub(crate) fn fini_fn(&self) -> Option<extern "C" fn()> {
        self.fini_fn
    }

    pub(crate) fn fini_array_fn(&self) -> Option<&[extern "C" fn()]> {
        self.fini_array_fn
    }
}
