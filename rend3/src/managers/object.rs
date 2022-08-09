use std::{
    any::TypeId,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    managers::{MaterialKeyPair, MaterialManager, MeshManager},
    types::{Object, ObjectHandle},
    util::{frustum::BoundingSphere, registry::ArchetypicalRegistry},
};
use glam::{Mat4, Vec3A};
use rend3_types::{Material, MaterialHandle, ObjectChange, ObjectMeshKind, RawObjectHandle};
use smallvec::SmallVec;

use super::SkeletonManager;

/// Cpu side input to gpu-based culling
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct GpuCullingInput {
    pub start_idx: u32,
    pub count: u32,
    pub vertex_offset: i32,
    pub material_index: u32,
    pub transform: Mat4,
    // xyz position; w radius
    pub bounding_sphere: BoundingSphere,
}

unsafe impl bytemuck::Pod for GpuCullingInput {}
unsafe impl bytemuck::Zeroable for GpuCullingInput {}

/// Internal representation of a Object.
#[repr(C, align(16))]
#[derive(Debug, Clone)]
pub struct InternalObject {
    pub mesh_kind: ObjectMeshKind,
    pub material_handle: MaterialHandle,
    // Index into the material archetype array
    pub location: Vec3A,
    pub input: GpuCullingInput,
}

impl InternalObject {
    pub fn mesh_location(&self) -> Vec3A {
        self.location + Vec3A::from(self.input.bounding_sphere.center)
    }
}

/// Manages objects. That's it. ¯\\\_(ツ)\_/¯
pub struct ObjectManager {
    registry: ArchetypicalRegistry<MaterialKeyPair, InternalObject, Object>,
}
impl ObjectManager {
    pub fn new() -> Self {
        profiling::scope!("ObjectManager::new");

        let registry = ArchetypicalRegistry::new();

        Self { registry }
    }

    pub fn allocate(counter: &AtomicUsize) -> ObjectHandle {
        let idx = counter.fetch_add(1, Ordering::Relaxed);

        ObjectHandle::new(idx)
    }

    pub fn fill(
        &mut self,
        handle: &ObjectHandle,
        object: Object,
        mesh_manager: &mut MeshManager,
        skeleton_manager: &SkeletonManager,
        material_manager: &mut MaterialManager,
    ) {
        let (internal_mesh, skeleton_ranges) = match &object.mesh_kind {
            ObjectMeshKind::Animated(skeleton) => {
                let skeleton = skeleton_manager.internal_data(skeleton.get_raw());
                let mesh = mesh_manager.internal_data(skeleton.mesh_handle.get_raw());
                (mesh, &*skeleton.overridden_attribute_ranges)
            }
            ObjectMeshKind::Static(mesh) => {
                let mesh = mesh_manager.internal_data(mesh.get_raw());
                (mesh, &[][..])
            }
        };

        let mut vertex_attribute_start_offsets: SmallVec<[_; 16]> = SmallVec::new();
        material_manager.get_attributes(object.material.get_raw(), |required, supported| {
            // Make sure all required attributes are in the mesh and the supported attribute list.
            for &&required_attribute in required {
                // We can just directly use the internal mesh, as every attribute in the skeleton is also in the mesh.
                let found_in_mesh = internal_mesh
                    .vertex_attribute_ranges
                    .iter()
                    .any(|&(id, _)| id == required_attribute);

                // Check that our required attributes are in the supported one.
                let found_in_supported = internal_mesh
                    .vertex_attribute_ranges
                    .iter()
                    .any(|&(id, _)| id == required_attribute);

                assert!(found_in_mesh);
                assert!(found_in_supported);
            }

            for &&supported_attribute in supported {
                // We first check the skeleton for the attribute's base offset.
                let found_start_offset = skeleton_ranges
                    .iter()
                    .find_map(|(id, range)| (*id == supported_attribute).then_some(range.start));

                if let Some(start_offset) = found_start_offset {
                    vertex_attribute_start_offsets.push(start_offset as u32);
                    continue;
                }

                // After the skeleton, check the mesh for non-overriden attributes.
                match internal_mesh.get_attribute(&supported_attribute) {
                    Some(range) => vertex_attribute_start_offsets.push(range.start as u32),
                    // If the attribute isn't there, push u32::MAX.
                    None => vertex_attribute_start_offsets.push(u32::MAX),
                }
            }
        });

        let bounding_sphere = internal_mesh.bounding_sphere;
        let index_range = internal_mesh.index_range.clone();

        let (material_key, object_list) = material_manager.get_material_key_and_objects(object.material.get_raw());
        object_list.push(handle.get_raw());

        let shader_object = InternalObject {
            location: object.transform.transform_point3a(Vec3A::ZERO),
            input: GpuCullingInput {
                material_index: material_manager.get_internal_index(object.material.get_raw()) as u32,
                transform: object.transform,
                bounding_sphere,
                start_idx: index_range.start as u32,
                count: (index_range.end - index_range.start) as u32,
                vertex_offset: vertex_range.start as i32,
            },
            material_handle: object.material,
            mesh_kind: object.mesh_kind,
        };

        self.registry.insert(handle, shader_object, material_key);

        todo!()
    }

    pub fn ready(&mut self, material_manager: &mut MaterialManager) {
        profiling::scope!("Object Manager Ready");
        self.registry.remove_all_dead(|handle, object| {
            // Remove from material list
            {
                let objects = material_manager.get_objects(object.material_handle.get_raw());
                let index = objects.iter().position(|v| v.idx == handle).unwrap();
                objects.swap_remove(index);
            }
        });
    }

    pub fn set_material_index(&mut self, handle: RawObjectHandle, index: usize) {
        let object = self.registry.get_value_mut(handle);
        object.input.material_index = index as u32;
    }

    pub fn set_key(&mut self, handle: RawObjectHandle, key: MaterialKeyPair) {
        self.registry.set_key(handle, key);
    }

    pub fn set_object_transform(&mut self, handle: RawObjectHandle, transform: Mat4) {
        let object = self.registry.get_value_mut(handle);
        object.input.transform = transform;
        object.location = transform.transform_point3a(Vec3A::ZERO)
    }

    pub fn get_objects<M: Material>(&self, key: u64) -> &[InternalObject] {
        self.registry
            .get_archetype_vector(&MaterialKeyPair {
                // TODO(material): unify a M -> TypeId method
                ty: TypeId::of::<M>(),
                key,
            })
            .unwrap_or(&[])
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
        let src_obj = self.registry.get_value_mut(src_handle.get_raw());
        let dst_obj = Object {
            mesh_kind: change.mesh_kind.unwrap_or_else(|| src_obj.mesh_kind.clone()),
            material: change.material.unwrap_or_else(|| src_obj.material_handle.clone()),
            transform: change.transform.unwrap_or(src_obj.input.transform),
        };
        self.fill(&dst_handle, dst_obj, mesh_manager, skeleton_manager, material_manager);
    }
}

impl Default for ObjectManager {
    fn default() -> Self {
        Self::new()
    }
}
