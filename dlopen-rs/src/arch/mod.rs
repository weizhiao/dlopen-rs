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
