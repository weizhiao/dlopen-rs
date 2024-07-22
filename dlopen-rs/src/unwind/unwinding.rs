impl Drop for ELFUnwind {
    fn drop(&mut self) {
        use eh_finder::EH_FINDER;
        use hashbrown::hash_table::Entry;
        let mut eh_finder = unsafe { EH_FINDER.eh_info.write() };
        if let Entry::Occupied(entry) = eh_finder.entry(
            self.eh_frame_hdr as u64,
            |val| val.eh_frame_hdr == self.eh_frame_hdr,
            ELFUnwind::hasher,
        ) {
            let info = entry.remove();
            core::mem::forget(info.0);
        } else {
            unreachable!();
        };
    }
}

impl ELFUnwind{
    #[inline]
    unsafe fn register_unwind_info(unwind_info: &ELFUnwind) {
        use eh_finder::EH_FINDER;
        use hashbrown::hash_map::DefaultHashBuilder;

        EH_FINDER.eh_info.write().insert_unique(
            unwind_info.eh_frame_hdr as u64,
            unwind_info.clone(),
            ELFUnwind::hasher,
        );
    }

    #[cfg(feature = "unwinding")]
    //每个unwind_info的eh_frame_hdr都是不同的
    fn hasher(val: &ELFUnwind) -> u64 {
        val.eh_frame_hdr as u64
    }
}