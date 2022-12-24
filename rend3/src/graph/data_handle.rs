use std::{
    ops::Deref,
    sync::atomic::{AtomicUsize, Ordering},
};

use once_cell::sync::Lazy;

static DATA_HANDLE_INDEX_ALLOCATOR: AtomicUsize = AtomicUsize::new(0);

pub struct RenderGraphConnection {
    name: &'static str,
    index: Lazy<usize>,
}
impl RenderGraphConnection {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            index: Lazy::new(|| DATA_HANDLE_INDEX_ALLOCATOR.fetch_add(1, Ordering::Relaxed)),
        }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub fn index(&self) -> usize {
        *self.index
    }
}

impl Deref for RenderGraphConnection {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.index
    }
}
