use indexmap::map::IndexMap;
use rend3_types::{RawResourceHandle, ResourceHandle};
use std::{marker::PhantomData, sync::Weak};

use crate::util::typedefs::FastBuildHasher;

#[derive(Debug)]
struct ResourceStorage<T> {
    refcount: Weak<()>,
    data: T,
}

/// Registry that stores values in an IndexMap.
///
/// Used by many managers without special requirements.
#[derive(Debug)]
pub struct ResourceRegistry<T, HandleType> {
    mapping: IndexMap<usize, ResourceStorage<T>, FastBuildHasher>,
    _phantom: PhantomData<HandleType>,
}
impl<T, HandleType> ResourceRegistry<T, HandleType> {
    pub fn new() -> Self {
        Self {
            mapping: IndexMap::with_hasher(FastBuildHasher::default()),
            _phantom: PhantomData,
        }
    }

    pub fn insert(&mut self, handle: &ResourceHandle<HandleType>, data: T) {
        self.mapping.insert(
            handle.get_raw().idx,
            ResourceStorage {
                refcount: handle.get_weak_refcount(),
                data,
            },
        );
    }

    pub fn remove_all_dead(&mut self, mut func: impl FnMut(&mut Self, usize, T)) {
        profiling::scope!("ResourceRegistry::remove_all_dead");
        for idx in (0..self.mapping.len()).rev() {
            let element = self.mapping.get_index(idx).unwrap().1;
            if element.refcount.strong_count() == 0 {
                let (_, value) = self.mapping.swap_remove_index(idx).unwrap();
                func(self, idx, value.data)
            }
        }
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&usize, &T)> + Clone {
        self.mapping
            .iter()
            .map(|(idx, ResourceStorage { data, .. })| (idx, data))
    }

    pub fn values(&self) -> impl ExactSizeIterator<Item = &T> + Clone {
        self.mapping.values().map(|ResourceStorage { data, .. }| data)
    }

    pub fn values_mut(&mut self) -> impl ExactSizeIterator<Item = &mut T> {
        self.mapping.values_mut().map(|ResourceStorage { data, .. }| data)
    }

    pub fn get(&self, handle: RawResourceHandle<HandleType>) -> &T {
        &self.mapping.get(&handle.idx).unwrap().data
    }

    pub fn get_mut(&mut self, handle: RawResourceHandle<HandleType>) -> &mut T {
        &mut self.mapping.get_mut(&handle.idx).unwrap().data
    }

    pub fn get_index_of(&self, handle: RawResourceHandle<HandleType>) -> usize {
        self.mapping.get_index_of(&handle.idx).unwrap()
    }

    pub fn count(&self) -> usize {
        self.mapping.len()
    }
}

impl<T, HandleType> Default for ResourceRegistry<T, HandleType> {
    fn default() -> Self {
        Self::new()
    }
}
