use list_any::VecAny;
use rend3_types::{RawResourceHandle, ResourceHandle};
use std::{any::TypeId, marker::PhantomData, sync::Weak};

use crate::util::typedefs::FastHashMap;

/// Per-value data that isn't type erased in a [ArchitypicalErasedRegistry].
pub struct NonErasedData<Metadata> {
    pub handle: usize,
    pub refcount: Weak<()>,
    pub inner: Metadata,
}

#[derive(Clone, Copy)]
struct PerHandleData {
    index: usize,
    ty: TypeId,
}

/// Represents a single archetype in a [ArchitypicalErasedRegistry].
#[allow(clippy::type_complexity)]
pub struct Archetype<Metadata> {
    /// Archetype of Vec<T> where T is the type of the archetype.
    pub vec: VecAny,
    /// All data that doesn't need to be type erased. Uses the same indices as
    /// the vec.
    pub non_erased: Vec<NonErasedData<Metadata>>,
    remove_single: fn(
        &mut VecAny,
        &mut Vec<NonErasedData<Metadata>>,
        &mut FastHashMap<usize, PerHandleData>,
        usize,
        &mut dyn FnMut(&Metadata, usize),
    ) -> Metadata,
    remove_all_dead: fn(
        &mut VecAny,
        &mut Vec<NonErasedData<Metadata>>,
        &mut FastHashMap<usize, PerHandleData>,
        &mut dyn FnMut(&Metadata, usize),
    ),
}

/// Registry that stores type-erased values in archetypes, each type
/// corresponding to an archetype.
///
/// This is used by the [MaterialManager](crate::managers::MaterialManager) to
/// store materials of any type.
pub struct ArchitypicalErasedRegistry<HandleType, Metadata> {
    archetype_map: FastHashMap<TypeId, Archetype<Metadata>>,
    handle_map: FastHashMap<usize, PerHandleData>,
    _phantom: PhantomData<HandleType>,
}

impl<HandleType, Metadata> ArchitypicalErasedRegistry<HandleType, Metadata> {
    pub fn new() -> Self {
        Self {
            archetype_map: FastHashMap::default(),
            handle_map: FastHashMap::default(),
            _phantom: PhantomData,
        }
    }

    pub fn ensure_archetype<T: Send + Sync + 'static>(&mut self) {
        let type_id = TypeId::of::<T>();
        self.archetype_map.entry(type_id).or_insert_with(|| Archetype {
            vec: VecAny::new::<T>(),
            non_erased: Vec::new(),
            remove_all_dead: remove_all_dead::<T, Metadata>,
            remove_single: remove_single::<T, Metadata>,
        });
    }

    pub fn insert<T: Send + Sync + 'static>(
        &mut self,
        handle: &ResourceHandle<HandleType>,
        data: T,
        metadata: Metadata,
    ) -> &mut Metadata {
        let type_id = TypeId::of::<T>();
        let archetype = self.archetype_map.entry(type_id).or_insert_with(|| Archetype {
            vec: VecAny::new::<T>(),
            non_erased: Vec::new(),
            remove_all_dead: remove_all_dead::<T, Metadata>,
            remove_single: remove_single::<T, Metadata>,
        });
        let mut vec = archetype.vec.downcast_mut::<T>().unwrap();

        let vec_index = vec.len();
        archetype.non_erased.push(NonErasedData {
            handle: handle.get_raw().idx,
            refcount: handle.get_weak_refcount(),
            inner: metadata,
        });
        vec.push(data);

        let handle_value = handle.get_raw().idx;
        self.handle_map.insert(
            handle_value,
            PerHandleData {
                index: vec_index,
                ty: type_id,
            },
        );

        &mut archetype.non_erased[vec_index].inner
    }

    pub fn update<T: Send + Sync + 'static>(
        &mut self,
        handle: &ResourceHandle<HandleType>,
        data: T,
        mut renormalize: impl FnMut(&Metadata, usize),
    ) -> bool {
        let per_handle_data = self.handle_map.get_mut(&handle.get_raw().idx).unwrap();
        let index = per_handle_data.index;
        let current_type_id = &mut per_handle_data.ty;
        let new_type_id = TypeId::of::<T>();

        let archetype = self.archetype_map.get_mut(current_type_id).unwrap();
        if *current_type_id == new_type_id {
            // We're just updating the data
            archetype.vec.downcast_slice_mut::<T>().unwrap()[index] = data;

            false
        } else {
            // We need to change archetype, so we clean up, then insert with the old handle.
            // We must clean up first, so the value in the index map is still accurate.
            let metadata = (archetype.remove_single)(
                &mut archetype.vec,
                &mut archetype.non_erased,
                &mut self.handle_map,
                index,
                &mut renormalize,
            );

            self.insert(handle, data, metadata);

            true
        }
    }

    pub fn remove_all_dead(&mut self, mut renormalize: impl FnMut(&Metadata, usize)) {
        profiling::scope!("ResourceRegistry::remove_all_dead");
        for archetype in self.archetype_map.values_mut() {
            (archetype.remove_all_dead)(
                &mut archetype.vec,
                &mut archetype.non_erased,
                &mut self.handle_map,
                &mut renormalize,
            );
        }
    }

    pub fn count(&self) -> usize {
        self.handle_map.len()
    }

    pub fn get_ref<T: Send + Sync + 'static>(&self, handle: RawResourceHandle<HandleType>) -> &T {
        &self.archetype_map[&TypeId::of::<T>()]
            .vec
            .downcast_slice::<T>()
            .unwrap()[self.handle_map[&handle.idx].index]
    }

    pub fn get_ref_full<T: Send + Sync + 'static>(&self, handle: RawResourceHandle<HandleType>) -> (&T, &Metadata) {
        let archetype = &self.archetype_map[&TypeId::of::<T>()];
        let index = self.handle_map[&handle.idx].index;
        let t_ref = &archetype.vec.downcast_slice::<T>().unwrap()[index];
        let meta_ref = &archetype.non_erased[index].inner;

        (t_ref, meta_ref)
    }

    pub fn get_ref_full_by_index<T: Send + Sync + 'static>(&self, index: usize) -> (&T, &Metadata) {
        let archetype = &self.archetype_map[&TypeId::of::<T>()];
        let t_ref = &archetype.vec.downcast_slice::<T>().unwrap()[index];
        let meta_ref = &archetype.non_erased[index].inner;

        (t_ref, meta_ref)
    }

    pub fn get_metadata_mut<T: Send + Sync + 'static>(
        &mut self,
        handle: RawResourceHandle<HandleType>,
    ) -> &mut Metadata {
        let archetype = self.archetype_map.get_mut(&TypeId::of::<T>()).unwrap();
        let index = self.handle_map[&handle.idx].index;
        &mut archetype.non_erased[index].inner
    }

    pub fn get_index(&self, handle: RawResourceHandle<HandleType>) -> usize {
        self.handle_map[&handle.idx].index
    }

    pub fn get_type_id(&self, handle: RawResourceHandle<HandleType>) -> TypeId {
        self.handle_map[&handle.idx].ty
    }

    pub fn get_archetype_mut(&mut self, ty: TypeId) -> &mut Archetype<Metadata> {
        self.archetype_map.get_mut(&ty).unwrap()
    }

    pub fn archetypes_mut(&mut self) -> impl ExactSizeIterator<Item = (TypeId, &mut VecAny)> {
        self.archetype_map.iter_mut().map(|(key, value)| (*key, &mut value.vec))
    }

    pub fn archetype_lengths(&self) -> impl ExactSizeIterator<Item = (TypeId, usize)> + '_ {
        self.archetype_map.iter().map(|(key, value)| (*key, value.vec.len()))
    }
}

impl<HandleType, Metadata> Default for ArchitypicalErasedRegistry<HandleType, Metadata> {
    fn default() -> Self {
        Self::new()
    }
}

fn remove_single<T: Send + Sync + 'static, Metadata>(
    vec_any: &mut VecAny,
    non_erased: &mut Vec<NonErasedData<Metadata>>,
    per_handle_map: &mut FastHashMap<usize, PerHandleData>,
    idx: usize,
    renormalize: &mut dyn FnMut(&Metadata, usize),
) -> Metadata {
    let mut vec = vec_any.downcast_mut::<T>().unwrap();

    vec.swap_remove(idx);
    let deleted_metadata = non_erased.swap_remove(idx);
    // We don't need to remove our value from the index map or the archetype map
    // because this is only called in the context of an update, where going to
    // update these values anyway.

    // If we swapped an element, update its value in the index map
    if let Some(metadata) = non_erased.get(idx) {
        per_handle_map.get_mut(&metadata.handle).unwrap().index = idx;
        renormalize(&metadata.inner, idx);
    }

    deleted_metadata.inner
}

fn remove_all_dead<T: Send + Sync + 'static, Metadata>(
    vec_any: &mut VecAny,
    non_erased: &mut Vec<NonErasedData<Metadata>>,
    per_handle_map: &mut FastHashMap<usize, PerHandleData>,
    renormalize: &mut dyn FnMut(&Metadata, usize),
) {
    let mut vec = vec_any.downcast_mut::<T>().unwrap();

    profiling::scope!(&format!(
        "ArchitypicalRegistry::<{}>::remove_all_dead",
        std::any::type_name::<T>()
    ));

    assert_eq!(vec.len(), non_erased.len());
    for idx in (0..vec.len()).rev() {
        // SAFETY: We're iterating back to front, removing no more than once per time,
        // so this is always valid.
        let _ = unsafe { vec.get_unchecked(idx) };
        let metadata = unsafe { non_erased.get_unchecked(idx) };
        if metadata.refcount.strong_count() == 0 {
            let _ = vec.swap_remove(idx);
            let old_metadata = non_erased.swap_remove(idx);
            per_handle_map.remove(&old_metadata.handle);

            // If we swapped an element, update its value in the index map
            if let Some(metadata) = non_erased.get(idx) {
                per_handle_map.get_mut(&metadata.handle).unwrap().index = idx;
                renormalize(&metadata.inner, idx);
            }
        }
    }
}
