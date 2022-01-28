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
        let (internal_mesh, vertex_range) = match &object.mesh_kind {
            ObjectMeshKind::Animated(skeleton) => {
                let skeleton = skeleton_manager.internal_data(skeleton.get_raw());
                let mesh = mesh_manager.internal_data_mut(skeleton.mesh_handle.get_raw());
                (mesh, skeleton.skeleton_vertex_range.clone())
            }
            ObjectMeshKind::Static(mesh) => {
                let mesh = mesh_manager.internal_data_mut(mesh.get_raw());
                let vertex_range = mesh.vertex_range.clone();
                (mesh, vertex_range)
            }
        };
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

    /// Objects contain cached data that stores vertex ranges for the gpu
    /// culling calls. When the vertex buffers are reallocated all this data is
    /// invalidated. This function needs to be called to fix it.
    pub fn fix_objects_after_realloc(&mut self, mesh_manager: &MeshManager, skeleton_manager: &SkeletonManager) {
        for object in self.registry.iter_all_values_mut() {
            match &object.mesh_kind {
                ObjectMeshKind::Animated(skeleton_handle) => {
                    let skeleton = skeleton_manager.internal_data(skeleton_handle.get_raw());
                    let mesh = mesh_manager.internal_data(skeleton.mesh_handle.get_raw());
                    object.input.start_idx = mesh.index_range.start as u32;
                    object.input.count = mesh.index_range.len() as u32;
                    object.input.vertex_offset = skeleton.skeleton_vertex_range.start as i32;
                }
                ObjectMeshKind::Static(mesh_handle) => {
                    let mesh = mesh_manager.internal_data(mesh_handle.get_raw());
                    object.input.start_idx = mesh.index_range.start as u32;
                    object.input.count = mesh.index_range.len() as u32;
                    object.input.vertex_offset = mesh.vertex_range.start as i32;
                }
            }
        }
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
