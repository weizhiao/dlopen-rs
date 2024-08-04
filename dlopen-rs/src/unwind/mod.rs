#[cfg(any(
    all(feature = "libgcc", feature = "unwinding"),
    all(feature = "libgcc", feature = "libunwind"),
    all(feature = "unwinding", feature = "libunwind")
))]
compile_error!("only one unwind lib can be used");

cfg_if::cfg_if! {
    if #[cfg(feature = "libgcc")]{
        #[path ="libgcc.rs"]
        mod imp;
    }else if #[cfg(feature = "libunwind")]{
        #[path ="libunwind.rs"]
        mod imp;
    }else if #[cfg(feature = "unwinding")]{
        #[path ="unwinding.rs"]
        mod imp;
    }else{
        #[path ="dummy.rs"]
        mod imp;
    }
}

pub(crate) use imp::ELFUnwind;