use indexmap::map::IndexMap;
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

use crate::util::typedefs::{FastBuildHasher, FastHashMap};

#[derive(Debug)]
struct ResourceStorage<T> {
    refcount: Weak<()>,
    data: T,
}

#[derive(Debug)]
pub struct ResourceRegistry<T, HandleType> {
    mapping: IndexMap<usize, ResourceStorage<T>, FastBuildHasher>,
    current_idx: AtomicUsize,
    _phantom: PhantomData<HandleType>,
}
impl<T, HandleType> ResourceRegistry<T, HandleType> {
    pub fn new() -> Self {
        Self {
            mapping: IndexMap::with_hasher(FastBuildHasher::default()),
            current_idx: AtomicUsize::new(0),
            _phantom: PhantomData,
        }
    }

    pub fn allocate(&self) -> ResourceHandle<HandleType> {
        let idx = self.current_idx.fetch_add(1, Ordering::Relaxed);

        ResourceHandle::new(idx)
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

pub struct ArchitypeResourceStorage<T> {
    pub handle: usize,
    pub refcount: Weak<()>,
    pub data: T,
}

struct Architype {
    vec: VecAny,
    remove_all_dead: fn(&mut VecAny, &mut FastHashMap<usize, usize>),
}

pub struct ArchitypicalRegistry<HandleType> {
    architype_map: FastHashMap<TypeId, Architype>,
    index_map: FastHashMap<usize, usize>,
    current_idx: AtomicUsize,
    _phantom: PhantomData<HandleType>,
}

impl<HandleType> ArchitypicalRegistry<HandleType> {
    pub fn new() -> Self {
        Self {
            architype_map: FastHashMap::default(),
            index_map: FastHashMap::default(),
            current_idx: AtomicUsize::new(0),
            _phantom: PhantomData,
        }
    }

    pub fn allocate(&self) -> ResourceHandle<HandleType> {
        let idx = self.current_idx.fetch_add(1, Ordering::Relaxed);

        ResourceHandle::new(idx)
    }

    pub fn insert<T: Send + Sync + 'static>(&mut self, handle: &ResourceHandle<HandleType>, data: T) {
        let architype = self
            .architype_map
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Architype {
                vec: VecAny::new::<ArchitypeResourceStorage<T>>(),
                remove_all_dead: remove_all_dead::<T>,
            });
        let mut vec = architype.vec.downcast_mut::<ArchitypeResourceStorage<T>>().unwrap();

        let vec_index = vec.len();
        vec.push(ArchitypeResourceStorage {
            handle: handle.get_raw().idx,
            refcount: handle.get_weak_refcount(),
            data,
        });

        self.index_map.insert(handle.get_raw().idx, vec_index);
    }

    pub fn remove_all_dead(&mut self) {
        profiling::scope!("ResourceRegistry::remove_all_dead");
        for architype in self.architype_map.values_mut() {
            (architype.remove_all_dead)(&mut architype.vec, &mut self.index_map)
        }
    }

    pub fn get_ref<T: Send + Sync + 'static>(&self, handle: RawResourceHandle<HandleType>) -> &T {
        &self.architype_map[&TypeId::of::<T>()]
            .vec
            .downcast_slice::<ArchitypeResourceStorage<T>>()
            .unwrap()[self.index_map[&handle.idx]]
            .data
    }

    pub fn get_index(&self, handle: RawResourceHandle<HandleType>) -> usize {
        self.index_map[&handle.idx]
    }

    pub fn architypes_mut(&mut self) -> impl ExactSizeIterator<Item = (TypeId, &mut VecAny)> {
        self.architype_map.iter_mut().map(|(key, value)| (*key, &mut value.vec))
    }

    pub fn architype_lengths(&self) -> impl ExactSizeIterator<Item = (TypeId, usize)> + '_ {
        self.architype_map.iter().map(|(key, value)| (*key, value.vec.len()))
    }
}

fn remove_all_dead<T: Send + Sync + 'static>(vec_any: &mut VecAny, index_map: &mut FastHashMap<usize, usize>) {
    let mut vec = vec_any.downcast_mut::<ArchitypeResourceStorage<T>>().unwrap();

    profiling::scope!(&format!(
        "ArchitypicalRegistry::<{}>::remove_all_dead",
        std::any::type_name::<T>()
    ));
    for idx in (0..vec.len()).rev() {
        // SAFETY: We're iterating back to front, removing no more than once per time, so this is always valid.
        let element = unsafe { vec.get_unchecked(idx) };
        if element.refcount.strong_count() == 0 {
            vec.swap_remove(idx);
            // If we swapped an element, update its value in the index map
            if let Some(resource) = vec.get(idx) {
                *index_map.get_mut(&resource.handle).unwrap() = idx;
            }
        }
    }
}
