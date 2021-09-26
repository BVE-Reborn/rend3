use list_any::VecAny;
use rend3_types::{RawResourceHandle, ResourceHandle};
use std::{
    any::TypeId,
    marker::PhantomData,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Weak,
    },
};

use crate::util::typedefs::FastHashMap;

pub struct ArchetypeResourceStorage<T> {
    pub handle: usize,
    pub refcount: Weak<()>,
    pub data: T,
}

struct Archetype {
    vec: VecAny,
    remove_single: fn(&mut VecAny, usize, &mut FastHashMap<usize, usize>),
    remove_all_dead: fn(&mut VecAny, &mut FastHashMap<usize, usize>, &mut FastHashMap<usize, TypeId>),
}

pub struct ArchitypicalErasedRegistry<HandleType> {
    archetype_map: FastHashMap<TypeId, Archetype>,
    index_map: FastHashMap<usize, usize>,
    handle_archetype_map: FastHashMap<usize, TypeId>,
    current_idx: AtomicUsize,
    _phantom: PhantomData<HandleType>,
}

impl<HandleType> ArchitypicalErasedRegistry<HandleType> {
    pub fn new() -> Self {
        Self {
            archetype_map: FastHashMap::default(),
            index_map: FastHashMap::default(),
            handle_archetype_map: FastHashMap::default(),
            current_idx: AtomicUsize::new(0),
            _phantom: PhantomData,
        }
    }

    pub fn allocate(&self) -> ResourceHandle<HandleType> {
        let idx = self.current_idx.fetch_add(1, Ordering::Relaxed);

        ResourceHandle::new(idx)
    }

    pub fn insert<T: Send + Sync + 'static>(&mut self, handle: &ResourceHandle<HandleType>, data: T) {
        let type_id = TypeId::of::<T>();
        let archetype = self.archetype_map.entry(type_id).or_insert_with(|| Archetype {
            vec: VecAny::new::<ArchetypeResourceStorage<T>>(),
            remove_all_dead: remove_all_dead::<T>,
            remove_single: remove_single::<T>,
        });
        let mut vec = archetype.vec.downcast_mut::<ArchetypeResourceStorage<T>>().unwrap();

        let vec_index = vec.len();
        vec.push(ArchetypeResourceStorage {
            handle: handle.get_raw().idx,
            refcount: handle.get_weak_refcount(),
            data,
        });

        let handle_value = handle.get_raw().idx;
        self.index_map.insert(handle_value, vec_index);
        self.handle_archetype_map.insert(handle_value, type_id);
    }

    pub fn update<T: Send + Sync + 'static>(&mut self, handle: &ResourceHandle<HandleType>, data: T) -> bool {
        let current_type_id = self.handle_archetype_map.get_mut(&handle.get_raw().idx).unwrap();
        let new_type_id = TypeId::of::<T>();

        let archetype = self.archetype_map.get_mut(current_type_id).unwrap();
        if *current_type_id == new_type_id {
            // We're just updating the data
            archetype
                .vec
                .downcast_slice_mut::<ArchetypeResourceStorage<T>>()
                .unwrap()[self.index_map[&handle.get_raw().idx]]
                .data = data;

            false
        } else {
            // We need to change archetype, so we clean up, then insert with the old handle. We must clean up first, so the value in the index map is still accurate.
            (archetype.remove_single)(
                &mut archetype.vec,
                self.index_map[&handle.get_raw().idx],
                &mut self.index_map,
            );

            self.insert(handle, data);

            true
        }
    }

    pub fn remove_all_dead(&mut self) {
        profiling::scope!("ResourceRegistry::remove_all_dead");
        for archetype in self.archetype_map.values_mut() {
            (archetype.remove_all_dead)(&mut archetype.vec, &mut self.index_map, &mut self.handle_archetype_map);
        }
    }

    pub fn get_ref<T: Send + Sync + 'static>(&self, handle: RawResourceHandle<HandleType>) -> &T {
        &self.archetype_map[&TypeId::of::<T>()]
            .vec
            .downcast_slice::<ArchetypeResourceStorage<T>>()
            .unwrap()[self.index_map[&handle.idx]]
            .data
    }

    pub fn get_index(&self, handle: RawResourceHandle<HandleType>) -> usize {
        self.index_map[&handle.idx]
    }

    pub fn get_type_id(&self, handle: RawResourceHandle<HandleType>) -> TypeId {
        self.handle_archetype_map[&handle.idx]
    }

    pub fn get_archetype_vector(&self, ty: TypeId) -> &VecAny {
        &self.archetype_map[&ty].vec
    }

    pub fn archetypes_mut(&mut self) -> impl ExactSizeIterator<Item = (TypeId, &mut VecAny)> {
        self.archetype_map.iter_mut().map(|(key, value)| (*key, &mut value.vec))
    }

    pub fn archetype_lengths(&self) -> impl ExactSizeIterator<Item = (TypeId, usize)> + '_ {
        self.archetype_map.iter().map(|(key, value)| (*key, value.vec.len()))
    }
}

impl<HandleType> Default for ArchitypicalErasedRegistry<HandleType> {
    fn default() -> Self {
        Self::new()
    }
}

fn remove_single<T: Send + Sync + 'static>(
    vec_any: &mut VecAny,
    idx: usize,
    index_map: &mut FastHashMap<usize, usize>,
) {
    let mut vec = vec_any.downcast_mut::<ArchetypeResourceStorage<T>>().unwrap();

    vec.swap_remove(idx);
    // We don't need to remove our value from the index map or the archetype map because
    // this is only called in the context of an update, where going to update these values anyway.

    // If we swapped an element, update its value in the index map
    if let Some(resource) = vec.get(idx) {
        *index_map.get_mut(&resource.handle).unwrap() = idx;
    }
}

fn remove_all_dead<T: Send + Sync + 'static>(
    vec_any: &mut VecAny,
    index_map: &mut FastHashMap<usize, usize>,
    handle_archetype_map: &mut FastHashMap<usize, TypeId>,
) {
    let mut vec = vec_any.downcast_mut::<ArchetypeResourceStorage<T>>().unwrap();

    profiling::scope!(&format!(
        "ArchitypicalRegistry::<{}>::remove_all_dead",
        std::any::type_name::<T>()
    ));
    for idx in (0..vec.len()).rev() {
        // SAFETY: We're iterating back to front, removing no more than once per time, so this is always valid.
        let element = unsafe { vec.get_unchecked(idx) };
        if element.refcount.strong_count() == 0 {
            let old = vec.swap_remove(idx);
            index_map.remove(&old.handle);
            handle_archetype_map.remove(&old.handle);

            // If we swapped an element, update its value in the index map
            if let Some(resource) = vec.get(idx) {
                *index_map.get_mut(&resource.handle).unwrap() = idx;
            }
        }
    }
}
