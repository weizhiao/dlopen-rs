use elf::file::Class;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "x86_64")]{
        mod x86_64;
        pub(crate) use x86_64::*;
    }else if #[cfg(target_arch = "x86")]{
        mod x86;
        pub(crate) use x86::*;
    }else if #[cfg(target_arch = "riscv64")]{
        mod riscv64;
        pub(crate) use riscv64::*;
    }else if #[cfg(target_arch="aarch64")]{
        mod aarch64;
        pub(crate) use aarch64::*;
    }
}

cfg_if::cfg_if! {
    if #[cfg(target_pointer_width = "64")]{
        pub(crate) const E_CLASS: Class = Class::ELF64;
        pub(crate) type Phdr = elf::segment::Elf64_Phdr;
        pub(crate) type Dyn = elf::dynamic::Elf64_Dyn;
        pub(crate) type Rela = elf::relocation::Elf64_Rela;
        pub(crate) type ELFSymbol = elf::symbol::Elf64_Sym;
        pub(crate) const REL_MASK: usize = 0xFFFFFFFF;
        pub(crate) const REL_BIT: usize = 32;
        pub(crate) const PHDR_SIZE: usize = core::mem::size_of::<elf::segment::Elf64_Phdr>();
        pub(crate) const EHDR_SIZE: usize = core::mem::size_of::<elf::file::Elf64_Ehdr>();
    }else{
        pub(crate) const E_CLASS: Class = Class::ELF32;
        pub(crate) type Phdr = elf::segment::Elf32_Phdr;
        pub(crate) type Dyn = elf::dynamic::Elf32_Dyn;
        pub(crate) type Rela = elf::relocation::Elf32_Rela;
        pub(crate) type ELFSymbol = elf::symbol::Elf32_Sym;
        pub(crate) const REL_MASK: usize = 0xFF;
        pub(crate) const REL_BIT: usize = 8;
        pub(crate) const PHDR_SIZE: usize = core::mem::size_of::<elf::segment::Elf32_Phdr>();
        pub(crate) const EHDR_SIZE: usize = core::mem::size_of::<elf::file::Elf32_Ehdr>();
    }
}
