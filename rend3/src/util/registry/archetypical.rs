use std::{hash::Hash, marker::PhantomData, sync::Weak};

use rend3_types::{RawResourceHandle, ResourceHandle};

use crate::util::typedefs::FastHashMap;

#[derive(Debug)]
struct ResourceMetadata {
    handle: usize,
    refcount: Weak<()>,
}

struct ArchetypeStorage<T> {
    data: Vec<T>,
    metadata: Vec<ResourceMetadata>,
}
impl<T> Default for ArchetypeStorage<T> {
    fn default() -> Self {
        Self {
            data: Default::default(),
            metadata: Default::default(),
        }
    }
}

struct HandleData<K> {
    key: K,
    index: usize,
}

/// Registry that stores the values in archetypes, each key representing an
/// archetype.
///
/// This is used by the [ObjectManager](crate::managers::ObjectManager) to store
/// objects. It uses the material archetype and a custom u64 as the key.
pub struct ArchetypicalRegistry<K, V, HandleType> {
    archetype_map: FastHashMap<K, ArchetypeStorage<V>>,
    handle_info: FastHashMap<usize, HandleData<K>>,
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
            _phantom: PhantomData,
        }
    }

    pub fn insert(&mut self, handle: &ResourceHandle<HandleType>, data: V, key: K) {
        let storage = self.archetype_map.entry(key).or_default();

        let index = storage.data.len();
        storage.data.push(data);
        storage.metadata.push(ResourceMetadata {
            handle: handle.get_raw().idx,
            refcount: handle.get_weak_refcount(),
        });

        self.handle_info.insert(handle.get_raw().idx, HandleData { key, index });
    }

    pub fn remove_all_dead(&mut self, mut remove_fn: impl FnMut(usize, V)) {
        for archetype in self.archetype_map.values_mut() {
            let length = archetype.data.len();
            debug_assert_eq!(length, archetype.metadata.len());
            for idx in (0..length).rev() {
                // SAFETY: We're iterating back to front, removing no more than once per time,
                // so this is always valid.
                let metadata = unsafe { archetype.metadata.get_unchecked(idx) };
                if metadata.refcount.strong_count() == 0 {
                    let data = archetype.data.swap_remove(idx);
                    let metadata = archetype.metadata.swap_remove(idx);
                    self.handle_info.remove(&metadata.handle);
                    remove_fn(metadata.handle, data);

                    // If we swapped an element, update its value in the index map
                    if let Some(resource) = archetype.metadata.get_mut(idx) {
                        self.handle_info.get_mut(&resource.handle).unwrap().index = idx;
                    }
                }
            }
        }
    }

    pub fn set_key(&mut self, handle: RawResourceHandle<HandleType>, key: K) {
        let handle_info_ref = self.handle_info.get_mut(&handle.idx).unwrap();
        let old_index = handle_info_ref.index;

        // remove it from the old archetype
        let old_archetype = self.archetype_map.get_mut(&handle_info_ref.key).unwrap();
        let data = old_archetype.data.swap_remove(old_index);
        let metadata = old_archetype.metadata.swap_remove(old_index);

        // If we swapped an element get its handle to update later. We can't update now
        // as handle_info_ref is holding a &mut on self.handle_info
        let update_handle = old_archetype
            .metadata
            .get_mut(old_index)
            .map(|resource| resource.handle);

        // Add it to the new archetype
        let new_archetype = self.archetype_map.entry(key).or_default();
        let new_index = new_archetype.data.len();
        new_archetype.data.push(data);
        new_archetype.metadata.push(metadata);

        // Update our metadata
        handle_info_ref.key = key;
        handle_info_ref.index = new_index;

        // Update the index if swap_remove moved an object.
        if let Some(handle) = update_handle {
            self.handle_info.get_mut(&handle).unwrap().index = old_index;
        }
    }

    pub fn count(&self) -> usize {
        self.handle_info.len()
    }

    pub fn get_value_mut(&mut self, handle: RawResourceHandle<HandleType>) -> &mut V {
        let handle_info = &self.handle_info[&handle.idx];
        &mut self.archetype_map.get_mut(&handle_info.key).unwrap().data[handle_info.index]
    }

    pub fn get_archetype_vector(&self, key: &K) -> Option<&[V]> {
        Some(&self.archetype_map.get(key)?.data)
    }

    /// Returns an iterator over all values regardless of its archetype
    pub fn iter_all_values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.archetype_map.values_mut().flat_map(|val| val.data.iter_mut())
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
