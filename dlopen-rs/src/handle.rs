use std::{marker::PhantomData, sync::Arc};

use crate::ELFLibrary;

#[derive(Debug, Clone)]
pub struct ELFHandle<'a> {
    inner: Arc<ELFLibrary>,
    phantom: PhantomData<&'a ELFLibrary>,
}

impl<'a> ELFHandle<'a> {
    pub(crate) fn new(inner: ELFLibrary) -> ELFHandle<'a> {
        ELFHandle {
            inner: Arc::new(inner),
            phantom: PhantomData,
        }
    }

    pub fn get_sym(&self, name: &str) -> Option<*const ()> {
        self.inner.get_sym(name)
    }
}
