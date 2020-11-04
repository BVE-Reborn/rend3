use crate::{
    bind_merge::BindGroupBuilder,
    datatypes::{AffineTransform, MaterialHandle, Object, ObjectHandle},
    registry::ResourceRegistry,
    renderer::{frustum::BoundingSphere, material::MaterialManager, mesh::MeshManager},
};
use smallvec::SmallVec;
use std::{mem::size_of, sync::Arc};
use wgpu::{BindGroupEntry, BufferAddress, BufferUsage, CommandEncoder, Device};
use wgpu_conveyor::{write_to_buffer2, AutomatedBuffer, AutomatedBufferManager, IdBuffer};

#[derive(Debug, Clone)]
struct InternalObject {
    materials: SmallVec<[MaterialHandle; 4]>,
    transform: AffineTransform,
    sphere: BoundingSphere,
    start_idx: u32,
    count: u32,
    vertex_offset: i32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
struct ShaderObject {
    start_idx: u32,
    count: u32,
    vertex_offset: i32,
    material_translation_idx: u32,
    transform: AffineTransform,
    sphere: BoundingSphere,
}

unsafe impl bytemuck::Zeroable for ShaderObject {}
unsafe impl bytemuck::Pod for ShaderObject {}

const SHADER_OBJECT_SIZE: usize = size_of::<ShaderObject>();
const MATERIAL_TRANSLATION_SIZE: usize = size_of::<u32>();

pub struct ObjectManager {
    object_info_buffer: AutomatedBuffer,
    object_info_buffer_storage: Option<Arc<IdBuffer>>,
    material_translation_buffer: AutomatedBuffer,
    material_translation_buffer_storage: Option<Arc<IdBuffer>>,
    material_translation_count: usize,

    registry: ResourceRegistry<InternalObject>,
}
impl ObjectManager {
    pub fn new(device: &Device, buffer_manager: &mut AutomatedBufferManager) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Object Manager");

        let object_info_buffer =
            buffer_manager.create_new_buffer(device, 0, BufferUsage::STORAGE, Some("object info buffer"));
        let material_translation_buffer =
            buffer_manager.create_new_buffer(device, 0, BufferUsage::STORAGE, Some("material translation buffer"));

        let registry = ResourceRegistry::new();

        Self {
            object_info_buffer,
            object_info_buffer_storage: None,
            material_translation_buffer,
            material_translation_buffer_storage: None,
            material_translation_count: 0,
            registry,
        }
    }

    pub fn allocate(&self) -> ObjectHandle {
        ObjectHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: ObjectHandle, object: Object, mesh_manager: &MeshManager) {
        span_transfer!(_ -> fill_span, INFO, "Object Manager Fill");

        let mesh = mesh_manager.internal_data(object.mesh);

        let material_count = object.materials.len();

        assert_eq!(
            material_count, mesh.material_count as usize,
            "Mismatching material and mesh material count. Material: {}, Mesh: {}",
            material_count, mesh.material_count
        );

        self.material_translation_count += material_count;

        let shader_object = InternalObject {
            materials: object.materials,
            transform: object.transform,
            sphere: mesh.bounding_sphere,
            start_idx: mesh.index_range.start as u32,
            count: (mesh.index_range.end - mesh.index_range.start) as u32,
            vertex_offset: mesh.vertex_range.start as i32,
        };

        self.registry.insert(handle.0, shader_object);
    }

    pub fn remove(&mut self, handle: ObjectHandle) {
        let (_, object) = self.registry.remove(handle.0);
        self.material_translation_count -= object.materials.len();
    }

    pub fn ready(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        material_manager: &MaterialManager,
    ) -> usize {
        span_transfer!(_ -> ready_span, INFO, "Object Manager Ready");

        let obj_buffer = &mut self.object_info_buffer;
        let mat_buffer = &mut self.material_translation_buffer;
        let registry = &self.registry;

        let object_count = self.registry.count();
        let mat_buffer_count = self.material_translation_count;

        let obj_buffer_size = (object_count * SHADER_OBJECT_SIZE) as BufferAddress;
        let mat_buffer_size = (mat_buffer_count * MATERIAL_TRANSLATION_SIZE) as BufferAddress;
        write_to_buffer2(
            device,
            encoder,
            obj_buffer,
            obj_buffer_size,
            mat_buffer,
            mat_buffer_size,
            |_, obj_slice, mat_slice| {
                let obj_slice: &mut [ShaderObject] = bytemuck::cast_slice_mut(obj_slice);
                let mat_slice: &mut [u32] = bytemuck::cast_slice_mut(mat_slice);

                let mut mat_start_offset: usize = 0;
                for (object_idx, object) in registry.values().enumerate() {
                    // Material Update

                    let mat_count = object.materials.len();
                    let mat_end_offset = mat_start_offset + mat_count;

                    let mat_slice = &mut mat_slice[mat_start_offset..mat_end_offset];

                    for (mat_idx, mat) in mat_slice.iter_mut().enumerate() {
                        *mat = material_manager.internal_index(object.materials[mat_idx]) as u32;
                    }

                    // Object Update

                    obj_slice[object_idx] = ShaderObject {
                        start_idx: object.start_idx,
                        count: object.count,
                        vertex_offset: object.vertex_offset,
                        material_translation_idx: mat_start_offset as u32,
                        transform: object.transform,
                        sphere: object.sphere,
                    };

                    // Prepare for next iteration

                    mat_start_offset += mat_count;
                }
            },
        );

        self.object_info_buffer_storage = Some(obj_buffer.get_current_inner());
        self.material_translation_buffer_storage = Some(mat_buffer.get_current_inner());

        object_count
    }

    pub fn append_to_bgb<'a>(&'a self, general_bgb: &mut BindGroupBuilder<'a>) {
        general_bgb.append(BindGroupEntry {
            binding: 0,
            resource: self
                .object_info_buffer_storage
                .as_ref()
                .unwrap()
                .inner
                .as_entire_binding(),
        });

        general_bgb.append(BindGroupEntry {
            binding: 0,
            resource: self
                .material_translation_buffer_storage
                .as_ref()
                .unwrap()
                .inner
                .as_entire_binding(),
        });
    }

    pub fn set_object_transform(&mut self, handle: ObjectHandle, transform: AffineTransform) {
        self.registry.get_mut(handle.0).transform = transform;
    }
}
