use std::{
    hash::Hash,
    marker::PhantomData,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Weak,
    },
};

use rend3_types::{RawResourceHandle, ResourceHandle};

use crate::util::typedefs::FastHashMap;

#[derive(Debug)]
struct ResourceStorage<T> {
    handle: usize,
    refcount: Weak<()>,
    data: T,
}

struct HandleData<K> {
    key: K,
    index: usize,
}

pub struct ArchetypicalRegistry<K, V, HandleType> {
    archetype_map: FastHashMap<K, Vec<ResourceStorage<V>>>,
    handle_info: FastHashMap<usize, HandleData<K>>,
    current_idx: AtomicUsize,
    _phantom: PhantomData<HandleType>,
}
impl<K, V, HandleType> ArchetypicalRegistry<K, V, HandleType>
where
    K: Copy + Eq + Hash,
{
    pub fn new() -> Self {
        Self {
            archetype_map: FastHashMap::default(),
            handle_info: FastHashMap::default(),
            current_idx: AtomicUsize::new(0),
            _phantom: PhantomData,
        }
    }

    pub fn allocate(&self) -> ResourceHandle<HandleType> {
        let idx = self.current_idx.fetch_add(1, Ordering::Relaxed);

        ResourceHandle::new(idx)
    }

    pub fn insert(&mut self, handle: &ResourceHandle<HandleType>, data: V, key: K) {
        let vec = self.archetype_map.entry(key).or_default();

        let index = vec.len();
        vec.push(ResourceStorage {
            handle: handle.get_raw().idx,
            refcount: handle.get_weak_refcount(),
            data,
        });

        self.handle_info.insert(handle.get_raw().idx, HandleData { key, index });
    }

    pub fn remove_all_dead(&mut self) {
        for archetype in self.archetype_map.values_mut() {
            for idx in (0..archetype.len()).rev() {
                // SAFETY: We're iterating back to front, removing no more than once per time, so this is always valid.
                let element = unsafe { archetype.get_unchecked(idx) };
                if element.refcount.strong_count() == 0 {
                    let value = archetype.swap_remove(idx);
                    self.handle_info.remove(&value.handle);

                    // If we swapped an element, update its value in the index map
                    if let Some(resource) = archetype.get_mut(idx) {
                        self.handle_info.get_mut(&resource.handle).unwrap().index = idx;
                    }
                }
            }
        }
    }

    pub fn get_value_mut(&mut self, handle: RawResourceHandle<HandleType>) -> &mut V {
        let handle_info = &self.handle_info[&handle.idx];
        &mut self.archetype_map.get_mut(&handle_info.key).unwrap()[handle_info.index].data
    }
}

impl<K, V, HandleType> Default for ArchetypicalRegistry<K, V, HandleType>
where
    K: Copy + Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}
