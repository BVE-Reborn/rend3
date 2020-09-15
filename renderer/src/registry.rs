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

    pub fn insert(&mut self, handle: usize, data: T) -> usize {
        self.mapping.insert_full(handle, data).0
    }

    pub fn remove(&mut self, handle: usize) -> (usize, T) {
        let (index, _key, value) = self.mapping.swap_remove_full(&handle).expect("Invalid handle");
        (index, value)
    }

    pub fn count(&self) -> usize {
        self.mapping.len()
    }
}
