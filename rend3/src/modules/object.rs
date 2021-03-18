use crate::{
    bind_merge::BindGroupBuilder,
    datatypes::{AffineTransform, MaterialHandle, Object, ObjectHandle},
    mode::ModeData,
    modules::MeshManager,
    registry::ResourceRegistry,
    renderer::modules::MaterialManager,
    util::frustum::BoundingSphere,
    RendererMode,
};
use std::{mem::size_of, sync::Arc};
use wgpu::{BufferAddress, BufferUsage, CommandEncoder, Device};
use wgpu_conveyor::{write_to_buffer1, AutomatedBuffer, AutomatedBufferManager, IdBuffer};

#[derive(Debug, Clone)]
pub struct InternalObject {
    pub material: MaterialHandle,
    pub transform: AffineTransform,
    pub sphere: BoundingSphere,
    pub start_idx: u32,
    pub count: u32,
    pub vertex_offset: i32,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
struct ShaderInputObject {
    start_idx: u32,
    count: u32,
    vertex_offset: i32,
    material_idx: u32,
    transform: AffineTransform,
    sphere: BoundingSphere,
}

unsafe impl bytemuck::Zeroable for ShaderInputObject {}
unsafe impl bytemuck::Pod for ShaderInputObject {}

const SHADER_OBJECT_SIZE: usize = size_of::<ShaderInputObject>();

pub struct ObjectManager {
    object_info_buffer: ModeData<(), AutomatedBuffer>,
    object_info_buffer_storage: ModeData<(), Option<Arc<IdBuffer>>>,

    registry: ResourceRegistry<InternalObject>,
}
impl ObjectManager {
    pub fn new(device: &Device, mode: RendererMode, buffer_manager: &mut AutomatedBufferManager) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Object Manager");

        let object_info_buffer = mode.into_data(
            || (),
            || buffer_manager.create_new_buffer(device, 0, BufferUsage::STORAGE, Some("object info buffer")),
        );

        let registry = ResourceRegistry::new();

        Self {
            object_info_buffer,
            object_info_buffer_storage: mode.into_data(|| (), || None),
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

        let object_count = self.registry.count();

        if object_count == 0 {
            return object_count;
        }

        if let ModeData::GPU(ref mut obj_buffer) = self.object_info_buffer {
            let registry = &self.registry;

            let obj_buffer_size = (object_count * SHADER_OBJECT_SIZE) as BufferAddress;
            write_to_buffer1(device, encoder, obj_buffer, obj_buffer_size, |_, obj_slice| {
                let obj_slice: &mut [ShaderInputObject] = bytemuck::cast_slice_mut(obj_slice);

                for (object_idx, object) in registry.values().enumerate() {
                    // Object Update

                    obj_slice[object_idx] = ShaderInputObject {
                        start_idx: object.start_idx,
                        count: object.count,
                        vertex_offset: object.vertex_offset,
                        material_idx: material_manager.internal_index(object.material) as u32,
                        transform: object.transform,
                        sphere: object.sphere,
                    };
                }
            });

            *self.object_info_buffer_storage.as_gpu_mut() = Some(obj_buffer.get_current_inner());
        }

        object_count
    }

    pub fn values(&self) -> impl Iterator<Item = &InternalObject> {
        self.registry.values()
    }

    pub fn gpu_append_to_bgb<'a>(&'a self, general_bgb: &mut BindGroupBuilder<'a>) {
        general_bgb.append(
            self.object_info_buffer_storage
                .as_gpu()
                .as_ref()
                .unwrap()
                .inner
                .as_entire_binding(),
        );
    }

    pub fn set_object_transform(&mut self, handle: ObjectHandle, transform: AffineTransform) {
        self.registry.get_mut(handle.0).transform = transform;
    }
}
