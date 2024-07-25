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