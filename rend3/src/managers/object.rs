use std::{any::TypeId, ops::Range};

use crate::{
    managers::{InternalMesh, MaterialManager, MeshManager},
    types::{Object, ObjectHandle},
    util::{frustum::BoundingSphere, typedefs::FastHashMap},
};
use encase::ShaderType;
use glam::{Mat4, Vec3A};
use list_any::VecAny;
use rend3_types::{
    Material, MaterialArray, MaterialHandle, ObjectChange, ObjectMeshKind, RawObjectHandle, VertexAttributeId,
};

use super::SkeletonManager;

/// Cpu side input to gpu-based culling
#[derive(ShaderType)]
pub struct GpuCullingInput<M: Material> {
    pub transform: Mat4,
    pub bounding_sphere: BoundingSphere,
    pub first_index: u32,
    pub index_count: u32,
    pub material_index: u32,
    pub vertex_attribute_start_offsets:
        <M::SupportedAttributeArrayType as MaterialArray<&'static VertexAttributeId>>::U32Array,
}

/// Internal representation of a Object.
pub struct InternalObject<M: Material> {
    pub mesh_kind: ObjectMeshKind,
    pub material_handle: MaterialHandle,
    pub location: Vec3A,
    pub input: GpuCullingInput<M>,
}

impl<M: Material> InternalObject<M> {
    pub fn mesh_location(&self) -> Vec3A {
        self.location + Vec3A::from(self.input.bounding_sphere.center)
    }
}

struct ObjectArchetype {
    /// Inner type is Option<InternalObject<M>>
    data_vec: VecAny,
    set_object_transform: fn(&VecAny, usize, Mat4),
    duplicate_object: fn(&VecAny, usize, ObjectChange) -> Object,
    remove: fn(&VecAny, usize),
}

/// Manages objects. That's it. ¯\\\_(ツ)\_/¯
pub struct ObjectManager {
    storage: FastHashMap<TypeId, ObjectArchetype>,
    handle_to_typeid: FastHashMap<RawObjectHandle, TypeId>,
}
impl ObjectManager {
    pub fn new() -> Self {
        profiling::scope!("ObjectManager::new");

        Self {
            storage: FastHashMap::default(),
            handle_to_typeid: FastHashMap::default(),
        }
    }

    fn ensure_archetype<M: Material>(&mut self) -> &mut ObjectArchetype {
        let type_id = TypeId::of::<M>();
        self.storage.entry(type_id).or_insert_with(|| ObjectArchetype {
            data_vec: VecAny::new::<Option<InternalObject<M>>>(),
            set_object_transform: set_object_transform::<M>,
            duplicate_object: duplicate_object::<M>,
            remove: remove::<M>,
        })
    }

    pub fn add(
        &mut self,
        handle: &ObjectHandle,
        object: Object,
        mesh_manager: &mut MeshManager,
        skeleton_manager: &SkeletonManager,
        material_manager: &mut MaterialManager,
    ) {
        let (internal_mesh, skeleton_ranges) = match &object.mesh_kind {
            ObjectMeshKind::Animated(skeleton) => {
                let skeleton = skeleton_manager.internal_data(**skeleton);
                let mesh = mesh_manager.internal_data(*skeleton.mesh_handle);
                (mesh, &*skeleton.overridden_attribute_ranges)
            }
            ObjectMeshKind::Static(mesh) => {
                let mesh = mesh_manager.internal_data(**mesh);
                (mesh, &[][..])
            }
        };

        material_manager.call_object_callback(
            *object.material,
            ObjectCallbackArgs {
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

        let storage = &self.storage[&type_id];

        (storage.set_object_transform)(&storage.data_vec, handle.idx, transform);
    }

    pub fn remove(&mut self, handle: RawObjectHandle) {
        let type_id = self.handle_to_typeid[&handle];

        let storage = &self.storage[&type_id];

        (storage.remove)(&storage.data_vec, handle.idx);
    }

    pub fn get_objects<M: Material>(&self) -> &[Option<InternalObject<M>>] {
        let type_id = TypeId::of::<M>();

        self.storage[&type_id]
            .data_vec
            .downcast_slice::<Option<InternalObject<M>>>()
            .unwrap()
    }

    pub fn duplicate_object(
        &mut self,
        src_handle: ObjectHandle,
        dst_handle: ObjectHandle,
        change: ObjectChange,
        mesh_manager: &mut MeshManager,
        skeleton_manager: &SkeletonManager,
        material_manager: &mut MaterialManager,
    ) {
        let type_id = self.handle_to_typeid[&*src_handle];

        let archetype = self.storage[&type_id];

        let dst_obj = (archetype.duplicate_object)(&archetype.data_vec, src_handle.idx, change);

        self.add(&dst_handle, dst_obj, mesh_manager, skeleton_manager, material_manager);
    }
}

impl Default for ObjectManager {
    fn default() -> Self {
        Self::new()
    }
}

fn set_object_transform<M: Material>(data: &VecAny, idx: usize, transform: Mat4) {
    let data_vec = data.downcast_slice::<Option<InternalObject<M>>>().unwrap();

    let object = data_vec[idx].as_ref().unwrap();
    object.input.transform = transform;
    object.location = transform.transform_point3a(Vec3A::ZERO)
}

fn duplicate_object<M: Material>(data: &VecAny, idx: usize, change: ObjectChange) -> Object {
    let data_vec = data.downcast_slice::<Option<InternalObject<M>>>().unwrap();

    let src_obj = data_vec[idx].as_ref().unwrap();

    Object {
        mesh_kind: change.mesh_kind.unwrap_or_else(|| src_obj.mesh_kind.clone()),
        material: change.material.unwrap_or_else(|| src_obj.material_handle.clone()),
        transform: change.transform.unwrap_or(src_obj.input.transform),
    }
}

fn remove<M: Material>(data: &VecAny, idx: usize) {
    let data_vec = data.downcast_slice::<Option<InternalObject<M>>>().unwrap();

    data_vec[idx] = None;
}

pub(super) struct ObjectCallbackArgs<'a> {
    manager: &'a mut ObjectManager,
    internal_mesh: &'a InternalMesh,
    skeleton_ranges: &'a [(VertexAttributeId, Range<u64>)],
    handle: RawObjectHandle,
    object: Object,
}

pub(super) fn object_callback<M: Material>(material: &M, args: ObjectCallbackArgs<'_>) {
    // Make sure all required attributes are in the mesh and the supported attribute list.
    for &required_attribute in M::required_attributes() {
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
        location: args.object.transform.transform_point3a(Vec3A::ZERO),
        input: GpuCullingInput {
            material_index: args.object.material.idx as u32,
            transform: args.object.transform,
            bounding_sphere,
            first_index: index_range.start as u32,
            index_count: (index_range.end - index_range.start) as u32,
            vertex_attribute_start_offsets,
        },
        material_handle: args.object.material,
        mesh_kind: args.object.mesh_kind,
    };

    let type_id = TypeId::of::<M>();

    args.manager.handle_to_typeid.insert(args.handle, type_id);
    let archetype = args.manager.ensure_archetype::<M>();

    let data_vec = archetype.data_vec.downcast_mut::<Option<InternalObject<M>>>().unwrap();
    data_vec[args.handle.idx] = Some(internal_object);
}
