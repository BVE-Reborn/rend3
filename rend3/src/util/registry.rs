use fnv::FnvBuildHasher;
use indexmap::map::IndexMap;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
pub struct ResourceRegistry<T> {
    mapping: IndexMap<usize, T, FnvBuildHasher>,
    current_idx: AtomicUsize,
}
impl<T> ResourceRegistry<T> {
    pub fn new() -> Self {
        Self {
            mapping: IndexMap::with_hasher(FnvBuildHasher::default()),
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

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&usize, &T)> + Clone {
        self.mapping.iter()
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &T> + Clone {
        self.mapping.values()
    }

    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut T> {
        self.mapping.values_mut()
    }

    pub fn get(&self, handle: usize) -> &T {
        self.mapping.get(&handle).unwrap()
    }

    pub fn get_mut(&mut self, handle: usize) -> &mut T {
        self.mapping.get_mut(&handle).unwrap()
    }

    pub fn get_index_of(&self, handle: usize) -> usize {
        self.mapping.get_index_of(&handle).unwrap()
    }

    pub fn count(&self) -> usize {
        self.mapping.len()
    }
}
