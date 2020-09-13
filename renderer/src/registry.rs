use indexmap::map::IndexMap;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct ResourceRegistry<T> {
    mapping: IndexMap<usize, T>,
    current_idx: AtomicUsize,
}
impl<T> ResourceRegistry<T> {
    pub fn new() -> Self {
        Self {
            mapping: IndexMap::new(),
            current_idx: AtomicUsize::new(0),
        }
    }

    pub fn allocate(&self) -> usize {
        self.current_idx.fetch_add(1, Ordering::Relaxed)
    }

    pub fn insert(&mut self, handle: usize, data: T) {
        self.mapping.insert(handle, data);
    }

    pub fn remove(&mut self, handle: usize) {
        self.mapping.remove(&handle);
    }
}
