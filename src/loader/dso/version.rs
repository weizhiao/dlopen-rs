use super::SymbolData;
use elf::abi;

/// The special GNU extension section .gnu.version_d has a section type of SHT_GNU_VERDEF
/// This section shall contain symbol version definitions. The number of entries
/// in this section shall be contained in the DT_VERDEFNUM entry of the Dynamic
/// Section .dynamic, and also the sh_info member of the section header.
/// The sh_link member of the section header shall point to the section that
/// contains the strings referenced by this section.
///
/// The .gnu.version_d section shall contain an array of VerDef structures
/// optionally followed by an array of VerDefAux structures.
#[repr(C)]
struct VerDef {
    /// Version revision. This field shall be set to 1.
    vd_version: u16,
    /// Version information flag bitmask.
    vd_flags: u16,
    /// VersionIndex value referencing the SHT_GNU_VERSYM section.
    vd_ndx: u16,
    /// Number of associated verdaux array entries.
    vd_cnt: u16,
    /// Version name hash value (ELF hash function).
    vd_hash: u32,
    /// Offset in bytes to a corresponding entry in an array of VerDefAux structures.
    vd_aux: u32,
    /// Offset to the next VerDef entry, in bytes.
    vd_next: u32,
}

#[repr(C)]
struct VerDefAux {
    /// Offset to the version or dependency name string in the linked string table, in bytes.
    vda_name: u32,
    /// Offset to the next VerDefAux entry, in bytes.
    vda_next: u32,
}

/// The GNU extension section .gnu.version_r has a section type of SHT_GNU_VERNEED.
/// This section contains required symbol version definitions. The number of
/// entries in this section shall be contained in the DT_VERNEEDNUM entry of the
/// Dynamic Section .dynamic and also the sh_info member of the section header.
/// The sh_link member of the section header shall point to the referenced
/// string table section.
///
/// The section shall contain an array of VerNeed structures optionally
/// followed by an array of VerNeedAux structures.
#[repr(C)]
struct VerNeed {
    /// Version of structure. This value is currently set to 1,
    /// and will be reset if the versioning implementation is incompatibly altered.
    vn_version: u16,
    /// Number of associated verneed array entries.
    vn_cnt: u16,
    /// Offset to the file name string in the linked string table, in bytes.
    vn_file: u32,
    /// Offset to a corresponding entry in the VerNeedAux array, in bytes.
    vn_aux: u32,
    /// Offset to the next VerNeed entry, in bytes.
    vn_next: u32,
}

/// Version Need Auxiliary Entries from the .gnu.version_r section
#[repr(C)]
struct VerNeedAux {
    /// Dependency name hash value (ELF hash function).
    vna_hash: u32,
    /// Dependency information flag bitmask.
    vna_flags: u16,
    /// VersionIndex value used in the .gnu.version symbol version array.
    vna_other: u16,
    /// Offset to the dependency name string in the linked string table, in bytes.
    vna_name: u32,
    /// Offset to the next vernaux entry, in bytes.
    vna_next: u32,
}

struct VersionIndex(u16);

impl VersionIndex {
    fn index(&self) -> u16 {
        self.0 & abi::VER_NDX_VERSION
    }
}

#[derive(Clone)]
struct VersionIndexTable {
    ptr: *const VersionIndex,
}

impl VersionIndexTable {
    fn get(&self, sym_idx: usize) -> &VersionIndex {
        unsafe { &*self.ptr.add(sym_idx) }
    }
}

struct VerNeedAuxIterator {
    ptr: *const VerNeedAux,
    count: usize,
    num: usize,
}

impl Iterator for VerNeedAuxIterator {
    type Item = VerNeedAux;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count < self.num {
            let verneed_aux = unsafe { self.ptr.read() };
            self.ptr = unsafe { self.ptr.add(1) };
            self.count += 1;
            Some(verneed_aux)
        } else {
            None
        }
    }
}

#[derive(Clone)]
struct VerNeedTable {
    ptr: *const VerNeed,
    num: usize,
}

struct VerNeedIterator {
    ptr: *const VerNeed,
    count: usize,
    num: usize,
}

impl Iterator for VerNeedIterator {
    type Item = (VerNeed, VerNeedAuxIterator);

    fn next(&mut self) -> Option<Self::Item> {
        if self.count < self.num {
            let verneed = unsafe { self.ptr.read() };
            let verneed_aux = VerNeedAuxIterator {
                ptr: unsafe { self.ptr.byte_add(verneed.vn_aux as usize) } as *const VerNeedAux,
                count: 0,
                num: verneed.vn_cnt as usize,
            };
            self.ptr = unsafe { self.ptr.byte_add(verneed.vn_next as usize) };
            self.count += 1;
            Some((verneed, verneed_aux))
        } else {
            None
        }
    }
}

impl IntoIterator for &VerNeedTable {
    type IntoIter = VerNeedIterator;
    type Item = (VerNeed, VerNeedAuxIterator);

    fn into_iter(self) -> Self::IntoIter {
        VerNeedIterator {
            ptr: self.ptr,
            count: 0,
            num: self.num,
        }
    }
}

#[derive(Clone)]
struct VerDefTable {
    ptr: *const VerDef,
    num: usize,
}

struct VerDefIterator {
    ptr: *const VerDef,
    count: usize,
    num: usize,
}

struct VerDefAuxIterator {
    ptr: *const VerDefAux,
    count: usize,
    num: usize,
}

impl Iterator for VerDefAuxIterator {
    type Item = VerDefAux;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count < self.num {
            let verdef_aux = unsafe { self.ptr.read() };
            self.ptr = unsafe { self.ptr.add(1) };
            self.count += 1;
            Some(verdef_aux)
        } else {
            None
        }
    }
}

impl Iterator for VerDefIterator {
    type Item = (VerDef, VerDefAuxIterator);

    fn next(&mut self) -> Option<Self::Item> {
        if self.count < self.num {
            let verdef = unsafe { self.ptr.read() };
            let verdef_aux = VerDefAuxIterator {
                ptr: unsafe { self.ptr.byte_add(verdef.vd_aux as usize) } as *const VerDefAux,
                count: 0,
                num: verdef.vd_cnt as usize,
            };
            self.ptr = unsafe { self.ptr.byte_add(verdef.vd_next as usize) };
            self.count += 1;
            Some((verdef, verdef_aux))
        } else {
            None
        }
    }
}

impl IntoIterator for &VerDefTable {
    type IntoIter = VerDefIterator;
    type Item = (VerDef, VerDefAuxIterator);

    fn into_iter(self) -> Self::IntoIter {
        VerDefIterator {
            ptr: self.ptr,
            count: 0,
            num: self.num,
        }
    }
}

#[derive(Clone)]
pub(crate) struct ELFVersion {
    version_ids: VersionIndexTable,
    verneeds: Option<VerNeedTable>,
    verdefs: Option<VerDefTable>,
}

impl ELFVersion {
    pub(crate) fn new(
        version_ids_off: usize,
        verneeds: Option<(usize, usize)>,
        verdefs: Option<(usize, usize)>,
    ) -> ELFVersion {
        ELFVersion {
            version_ids: VersionIndexTable {
                ptr: version_ids_off as _,
            },
            verneeds: verneeds.map(|(off, num)| VerNeedTable { ptr: off as _, num }),
            verdefs: verdefs.map(|(off, num)| VerDefTable { ptr: off as _, num }),
        }
    }
}

pub(crate) struct SymbolVersion<'a> {
    pub name: &'a str,
    pub hash: u32,
}

impl<'a> SymbolVersion<'a> {
    /// glibc:_dl_elf_hash
    fn dl_elf_hash(name: &str) -> u32 {
        let bytes = name.as_bytes();
        let mut hash: u32 = bytes[0] as u32;

        if hash != 0 && bytes.len() > 1 {
            hash = (hash << 4) + bytes[1] as u32;
            if bytes.len() > 2 {
                hash = (hash << 4) + bytes[2] as u32;
                if bytes.len() > 3 {
                    hash = (hash << 4) + bytes[3] as u32;
                    if bytes.len() > 4 {
                        hash = (hash << 4) + bytes[4] as u32;
                        let mut name = &bytes[5..];
                        while let Some(&byte) = name.first() {
                            hash = (hash << 4) + byte as u32;
                            let hi = hash & 0xf0000000;
                            hash ^= hi >> 24;
                            name = &name[1..];
                        }
                        hash &= 0x0fffffff;
                    }
                }
            }
        }
        hash
    }

    pub(crate) fn new(name: &'a str) -> Self {
        let hash = Self::dl_elf_hash(name);
        SymbolVersion { name, hash }
    }
}

impl SymbolData {
    pub(crate) fn get_requirement(&self, sym_idx: usize) -> Option<SymbolVersion> {
        if let Some(version) = &self.version {
            let ver_ndx = version.version_ids.get(sym_idx);
            if ver_ndx.index() <= 1 {
                return None;
            }
            if let Some(verneeds) = &version.verneeds {
                for (_, vna_iter) in verneeds {
                    for vna in vna_iter {
                        if vna.vna_other != ver_ndx.index() {
                            continue;
                        }
                        let name = self.strtab.get(vna.vna_name as usize);
                        let hash = vna.vna_hash;
                        return Some(SymbolVersion { name, hash });
                    }
                }
            }
        }
        None
    }

    pub(crate) fn check_match(&self, sym_idx: usize, version: &Option<SymbolVersion>) -> bool {
        if let Some(version) = version {
            let def_version = self.version.as_ref().unwrap();
            let verdefs = def_version.verdefs.as_ref().unwrap();
            let ver_ndx = def_version.version_ids.get(sym_idx);
            for (vd, vda_iter) in verdefs {
                if vd.vd_ndx != ver_ndx.index() {
                    continue;
                }
                let hash = vd.vd_hash;
                if hash == version.hash
                    && vda_iter
                        .into_iter()
                        .any(|vda| self.strtab.get(vda.vda_name as usize) == version.name)
                {
                    return true;
                }
                return false;
            }
            false
        } else {
            true
        }
    }
}
