use crate::{
    datatypes::{Material, MaterialHandle, TextureHandle},
    registry::ResourceRegistry,
    renderer::{limits::MAX_UNIFORM_BUFFER_BINDING_SIZE, texture::TextureManager},
};
use std::{mem::size_of, num::NonZeroU32};
use wgpu::{BufferUsage, CommandEncoder, Device};
use wgpu_conveyor::{AutomatedBuffer, AutomatedBufferManager};

const MAX_MATERIALS: usize = MAX_UNIFORM_BUFFER_BINDING_SIZE as usize / size_of::<ShaderMaterial>();

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct ShaderMaterial {
    color: Option<NonZeroU32>,
    normal: Option<NonZeroU32>,
    roughness: Option<NonZeroU32>,
    specular: Option<NonZeroU32>,
}

unsafe impl bytemuck::Zeroable for ShaderMaterial {}
unsafe impl bytemuck::Pod for ShaderMaterial {}

pub struct MaterialManager {
    buffer: AutomatedBuffer,

    registry: ResourceRegistry<Material>,
}
impl MaterialManager {
    pub fn new(device: &Device, manager: &mut AutomatedBufferManager) -> Self {
        let buffer = manager.create_new_buffer(
            device,
            MAX_UNIFORM_BUFFER_BINDING_SIZE,
            BufferUsage::UNIFORM,
            Some("material buffer"),
        );
        let registry = ResourceRegistry::new();

        Self { buffer, registry }
    }

    pub fn allocate(&self) -> MaterialHandle {
        MaterialHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: MaterialHandle, material: Material) {
        self.registry.insert(handle.0, material);
    }

    pub fn remove(&mut self, handle: MaterialHandle) {
        self.registry.remove(handle.0);
    }

    pub fn ready(&mut self, device: &Device, encoder: &mut CommandEncoder, texture_manager: &TextureManager) {
        let registry = &self.registry;
        self.buffer
            .write_to_buffer(device, encoder, MAX_UNIFORM_BUFFER_BINDING_SIZE, |slice| {
                let typed_slice: &mut [ShaderMaterial] = bytemuck::cast_slice_mut(slice);

                let translate_texture = |v: TextureHandle| unsafe {
                    NonZeroU32::new_unchecked(texture_manager.internal_index(v) as u32 + 1)
                };

                for (index, material) in registry.values().enumerate() {
                    typed_slice[index] = ShaderMaterial {
                        color: material.color.map(translate_texture),
                        normal: material.normal.map(translate_texture),
                        roughness: material.roughness.map(translate_texture),
                        specular: material.specular.map(translate_texture),
                    }
                }
            });
    }

    pub fn current_buffer(&self) -> &AutomatedBuffer {
        &self.buffer
    }
}
