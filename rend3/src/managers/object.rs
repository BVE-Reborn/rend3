use std::{any::TypeId, ops::Range};

use encase::ShaderType;
use glam::{Mat4, Vec3A};
use list_any::VecAny;
use rend3_types::{
    Material, MaterialArray, MaterialHandle, ObjectChange, ObjectMeshKind, RawObjectHandle, VertexAttributeId,
};
use wgpu::{Buffer, CommandEncoder, Device};

use super::SkeletonManager;
use crate::{
    managers::{InternalMesh, MaterialManager, MeshManager},
    types::{Object, ObjectHandle},
    util::{
        freelist::FreelistDerivedBuffer, frustum::BoundingSphere, iter::ExactSizerIterator, scatter_copy::ScatterCopy,
        typedefs::FastHashMap,
    },
};

/// Cpu side input to gpu-based culling
#[derive(ShaderType)]
pub struct ShaderObject<M: Material> {
    pub transform: Mat4,
    pub bounding_sphere: BoundingSphere,
    pub first_index: u32,
    pub index_count: u32,
    pub material_index: u32,
    pub vertex_attribute_start_offsets:
        <M::SupportedAttributeArrayType as MaterialArray<&'static VertexAttributeId>>::U32Array,
}

// Manual impl so that M: !Copy
impl<M: Material> Copy for ShaderObject<M> {}

// Manual impl so that M: !Clone
impl<M: Material> Clone for ShaderObject<M> {
    fn clone(&self) -> Self {
        Self {
            transform: self.transform,
            bounding_sphere: self.bounding_sphere,
            first_index: self.first_index,
            index_count: self.index_count,
            material_index: self.material_index,
            vertex_attribute_start_offsets: self.vertex_attribute_start_offsets,
        }
    }
}

/// Internal representation of a Object.
pub struct InternalObject<M: Material> {
    pub mesh_kind: ObjectMeshKind,
    pub material_handle: MaterialHandle,
    pub location: Vec3A,
    pub inner: ShaderObject<M>,
}

// Manual impl so that M: !Clone
impl<M: Material> Clone for InternalObject<M> {
    fn clone(&self) -> Self {
        Self {
            mesh_kind: self.mesh_kind.clone(),
            material_handle: self.material_handle.clone(),
            location: self.location,
            inner: self.inner,
        }
    }
}

impl<M: Material> InternalObject<M> {
    pub fn mesh_location(&self) -> Vec3A {
        self.location + Vec3A::from(self.inner.bounding_sphere.center)
    }
}

struct ObjectArchetype {
    /// Inner type is Option<InternalObject<M>>
    data_vec: VecAny,
    object_count: usize,
    buffer: FreelistDerivedBuffer,
    set_object_transform: fn(&mut VecAny, &mut FreelistDerivedBuffer, usize, Mat4),
    duplicate_object: fn(&VecAny, usize, ObjectChange) -> Object,
    remove: fn(&mut VecAny, usize),
    evaluate: fn(&mut ObjectArchetype, &Device, &mut CommandEncoder, &ScatterCopy),
}

/// Manages objects. That's it. ¯\\\_(ツ)\_/¯
pub struct ObjectManager {
    archetype: FastHashMap<TypeId, ObjectArchetype>,
    handle_to_typeid: FastHashMap<RawObjectHandle, TypeId>,
}
impl ObjectManager {
    pub fn new() -> Self {
        profiling::scope!("ObjectManager::new");

        Self {
            archetype: FastHashMap::default(),
            handle_to_typeid: FastHashMap::default(),
        }
    }

    fn ensure_archetype<M: Material>(&mut self, device: &Device) -> &mut ObjectArchetype {
        let type_id = TypeId::of::<M>();
        self.archetype.entry(type_id).or_insert_with(|| ObjectArchetype {
            data_vec: VecAny::new::<Option<InternalObject<M>>>(),
            object_count: 0,
            buffer: FreelistDerivedBuffer::new::<ShaderObject<M>>(device),
            set_object_transform: set_object_transform::<M>,
            duplicate_object: duplicate_object::<M>,
            remove: remove::<M>,
            evaluate: evaluate::<M>,
        })
    }

    pub fn add(
        &mut self,
        device: &Device,
        handle: &ObjectHandle,
        object: Object,
        mesh_manager: &MeshManager,
        skeleton_manager: &SkeletonManager,
        material_manager: &mut MaterialManager,
    ) {
        let mesh_manager_guard = mesh_manager.lock_internal_data();
        let (internal_mesh, skeleton_ranges) = match &object.mesh_kind {
            ObjectMeshKind::Animated(skeleton) => {
                let skeleton = skeleton_manager.internal_data(**skeleton);
                let mesh = &mesh_manager_guard[*skeleton.mesh_handle];
                (mesh, &*skeleton.overridden_attribute_ranges)
            }
            ObjectMeshKind::Static(mesh) => {
                let mesh = &mesh_manager_guard[**mesh];
                (mesh, &[][..])
            }
        };

        material_manager.call_object_add_callback(
            *object.material,
            ObjectAddCallbackArgs {
                device,
                manager: self,
                internal_mesh,
                skeleton_ranges,
                handle: **handle,
                object,
            },
        );
    }

    pub fn set_object_transform(&mut self, handle: RawObjectHandle, transform: Mat4) {
        let type_id = self.handle_to_typeid[&handle];

        let archetype = self.archetype.get_mut(&type_id).unwrap();

        (archetype.set_object_transform)(&mut archetype.data_vec, &mut archetype.buffer, handle.idx, transform);
    }

    pub fn remove(&mut self, handle: RawObjectHandle) {
        let type_id = self.handle_to_typeid[&handle];

        let archetype = self.archetype.get_mut(&type_id).unwrap();

        (archetype.remove)(&mut archetype.data_vec, handle.idx);

        archetype.object_count -= 1;
    }

    pub fn evaluate(&mut self, device: &Device, encoder: &mut CommandEncoder, scatter: &ScatterCopy) {
        for archetype in self.archetype.values_mut() {
            (archetype.evaluate)(archetype, device, encoder, scatter);
        }
    }

    pub fn buffer<M: Material>(&self) -> Option<&Buffer> {
        Some(&self.archetype.get(&TypeId::of::<M>())?.buffer)
    }

    pub fn enumerated_objects<M: Material>(
        &self,
    ) -> Option<impl ExactSizeIterator<Item = (RawObjectHandle, &InternalObject<M>)> + '_> {
        let type_id = TypeId::of::<M>();

        let archetype = self.archetype.get(&type_id)?;

        let iter = archetype
            .data_vec
            .downcast_slice::<Option<InternalObject<M>>>()
            .unwrap()
            .iter()
            .enumerate()
            .filter_map(|(idx, o)| o.as_ref().map(|o| (RawObjectHandle::new(idx), o)));

        Some(ExactSizerIterator::new(iter, archetype.object_count))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn duplicate_object(
        &mut self,
        device: &Device,
        src_handle: ObjectHandle,
        dst_handle: ObjectHandle,
        change: ObjectChange,
        mesh_manager: &MeshManager,
        skeleton_manager: &SkeletonManager,
        material_manager: &mut MaterialManager,
    ) {
        let type_id = self.handle_to_typeid[&*src_handle];

        let archetype = self.archetype.get_mut(&type_id).unwrap();

        let dst_obj = (archetype.duplicate_object)(&mut archetype.data_vec, src_handle.idx, change);

        self.add(
            device,
            &dst_handle,
            dst_obj,
            mesh_manager,
            skeleton_manager,
            material_manager,
        );
    }
}

impl Default for ObjectManager {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) struct ObjectAddCallbackArgs<'a> {
    device: &'a Device,
    manager: &'a mut ObjectManager,
    internal_mesh: &'a InternalMesh,
    skeleton_ranges: &'a [(VertexAttributeId, Range<u64>)],
    handle: RawObjectHandle,
    object: Object,
}

pub(super) fn object_add_callback<M: Material>(_material: &M, args: ObjectAddCallbackArgs<'_>) {
    // Make sure all required attributes are in the mesh and the supported attribute list.
    for &required_attribute in M::required_attributes().into_iter() {
        // We can just directly use the internal mesh, as every attribute in the skeleton is also in the mesh.
        let found_in_mesh = args
            .internal_mesh
            .vertex_attribute_ranges
            .iter()
            .any(|&(id, _)| id == required_attribute);

        // Check that our required attributes are in the supported one.
        let found_in_supported = args
            .internal_mesh
            .vertex_attribute_ranges
            .iter()
            .any(|&(id, _)| id == required_attribute);

        assert!(found_in_mesh);
        assert!(found_in_supported);
    }

    let vertex_attribute_start_offsets = M::supported_attributes().map_to_u32(|&supported_attribute| {
        // We first check the skeleton for the attribute's base offset.
        let found_start_offset = args
            .skeleton_ranges
            .iter()
            .find_map(|(id, range)| (*id == supported_attribute).then_some(range.start));

        if let Some(start_offset) = found_start_offset {
            return start_offset as u32;
        }

        // After the skeleton, check the mesh for non-overriden attributes.
        match args.internal_mesh.get_attribute(&supported_attribute) {
            Some(range) => range.start as u32,
            // If the attribute isn't there, push u32::MAX.
            None => u32::MAX,
        }
    });

    let bounding_sphere = args.internal_mesh.bounding_sphere;
    let index_range = args.internal_mesh.index_range.clone();

    let internal_object = InternalObject::<M> {
        location: args.object.transform.transform_point3a(bounding_sphere.center.into()),
        inner: ShaderObject {
            material_index: args.object.material.idx as u32,
            transform: args.object.transform,
            bounding_sphere,
            first_index: (index_range.start / 4) as u32,
            index_count: ((index_range.end - index_range.start) / 4) as u32,
            vertex_attribute_start_offsets,
        },
        material_handle: args.object.material,
        mesh_kind: args.object.mesh_kind,
    };

    let type_id = TypeId::of::<M>();

    args.manager.handle_to_typeid.insert(args.handle, type_id);
    let archetype = args.manager.ensure_archetype::<M>(args.device);

    let mut data_vec = archetype.data_vec.downcast_mut::<Option<InternalObject<M>>>().unwrap();
    if args.handle.idx >= data_vec.len() {
        data_vec.resize_with((args.handle.idx + 1).next_power_of_two(), || None);
    }
    data_vec[args.handle.idx] = Some(internal_object);
    archetype.object_count += 1;
    archetype.buffer.use_index(args.handle.idx);
}

fn set_object_transform<M: Material>(
    data: &mut VecAny,
    buffer: &mut FreelistDerivedBuffer,
    idx: usize,
    transform: Mat4,
) {
    let data_vec = data.downcast_slice_mut::<Option<InternalObject<M>>>().unwrap();

    let object = data_vec[idx].as_mut().unwrap();
    object.inner.transform = transform;
    object.location = transform.transform_point3a(Vec3A::ZERO);

    buffer.use_index(idx);
}

fn duplicate_object<M: Material>(data: &VecAny, idx: usize, change: ObjectChange) -> Object {
    let data_vec = data.downcast_slice::<Option<InternalObject<M>>>().unwrap();

    let src_obj = data_vec[idx].as_ref().unwrap();

    Object {
        mesh_kind: change.mesh_kind.unwrap_or_else(|| src_obj.mesh_kind.clone()),
        material: change.material.unwrap_or_else(|| src_obj.material_handle.clone()),
        transform: change.transform.unwrap_or(src_obj.inner.transform),
    }
}

fn remove<M: Material>(data: &mut VecAny, idx: usize) {
    let data_vec = data.downcast_slice_mut::<Option<InternalObject<M>>>().unwrap();

    data_vec[idx] = None;
}

fn evaluate<M: Material>(
    archetype: &mut ObjectArchetype,
    device: &Device,
    encoder: &mut CommandEncoder,
    scatter: &ScatterCopy,
) {
    let data_vec = archetype
        .data_vec
        .downcast_slice::<Option<InternalObject<M>>>()
        .unwrap();

    archetype
        .buffer
        .apply(device, encoder, scatter, |idx| data_vec[idx].as_ref().unwrap().inner)
}
