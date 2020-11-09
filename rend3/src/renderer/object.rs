use crate::{
    bind_merge::BindGroupBuilder,
    datatypes::{AffineTransform, MaterialHandle, Object, ObjectHandle},
    registry::ResourceRegistry,
    renderer::{frustum::BoundingSphere, material::MaterialManager, mesh::MeshManager},
};
use std::{mem::size_of, sync::Arc};
use wgpu::{BindGroupEntry, BufferAddress, BufferUsage, CommandEncoder, Device};
use wgpu_conveyor::{write_to_buffer1, AutomatedBuffer, AutomatedBufferManager, IdBuffer};

#[derive(Debug, Clone)]
struct InternalObject {
    material: MaterialHandle,
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
    material_idx: u32,
    transform: AffineTransform,
    sphere: BoundingSphere,
}

unsafe impl bytemuck::Zeroable for ShaderObject {}
unsafe impl bytemuck::Pod for ShaderObject {}

const SHADER_OBJECT_SIZE: usize = size_of::<ShaderObject>();

pub struct ObjectManager {
    object_info_buffer: AutomatedBuffer,
    object_info_buffer_storage: Option<Arc<IdBuffer>>,

    registry: ResourceRegistry<InternalObject>,
}
impl ObjectManager {
    pub fn new(device: &Device, buffer_manager: &mut AutomatedBufferManager) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Object Manager");

        let object_info_buffer =
            buffer_manager.create_new_buffer(device, 0, BufferUsage::STORAGE, Some("object info buffer"));

        let registry = ResourceRegistry::new();

        Self {
            object_info_buffer,
            object_info_buffer_storage: None,
            registry,
        }
    }

    pub fn allocate(&self) -> ObjectHandle {
        ObjectHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: ObjectHandle, object: Object, mesh_manager: &MeshManager) {
        span_transfer!(_ -> fill_span, INFO, "Object Manager Fill");

        let mesh = mesh_manager.internal_data(object.mesh);

        let shader_object = InternalObject {
            material: object.material,
            transform: object.transform,
            sphere: mesh.bounding_sphere,
            start_idx: mesh.index_range.start as u32,
            count: (mesh.index_range.end - mesh.index_range.start) as u32,
            vertex_offset: mesh.vertex_range.start as i32,
        };

        self.registry.insert(handle.0, shader_object);
    }

    pub fn remove(&mut self, handle: ObjectHandle) {
        self.registry.remove(handle.0);
    }

    pub fn ready(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        material_manager: &MaterialManager,
    ) -> usize {
        span_transfer!(_ -> ready_span, INFO, "Object Manager Ready");

        let obj_buffer = &mut self.object_info_buffer;
        let registry = &self.registry;

        let object_count = self.registry.count();

        let obj_buffer_size = (object_count * SHADER_OBJECT_SIZE) as BufferAddress;
        write_to_buffer1(device, encoder, obj_buffer, obj_buffer_size, |_, obj_slice| {
            let obj_slice: &mut [ShaderObject] = bytemuck::cast_slice_mut(obj_slice);

            for (object_idx, object) in registry.values().enumerate() {
                // Object Update

                obj_slice[object_idx] = ShaderObject {
                    start_idx: object.start_idx,
                    count: object.count,
                    vertex_offset: object.vertex_offset,
                    material_idx: material_manager.internal_index(object.material) as u32,
                    transform: object.transform,
                    sphere: object.sphere,
                };
            }
        });

        self.object_info_buffer_storage = Some(obj_buffer.get_current_inner());

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
    }

    pub fn set_object_transform(&mut self, handle: ObjectHandle, transform: AffineTransform) {
        self.registry.get_mut(handle.0).transform = transform;
    }
}
