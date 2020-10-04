use crate::{
    datatypes::{AlbedoFlags, Material, MaterialHandle, TextureHandle},
    registry::ResourceRegistry,
    renderer::{limits::MAX_UNIFORM_BUFFER_BINDING_SIZE, texture::TextureManager},
};
use glam::f32::Vec4;
use std::{mem::size_of, num::NonZeroU32};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, BufferAddress, BufferUsage,
    CommandEncoder, Device,
};
use wgpu_conveyor::{AutomatedBuffer, AutomatedBufferManager, BindGroupCache, BufferCache1};

pub const MAX_MATERIALS: usize = MAX_UNIFORM_BUFFER_BINDING_SIZE as usize / size_of::<ShaderMaterial>();
pub const MATERIALS_SIZE: BufferAddress = (MAX_MATERIALS * size_of::<ShaderMaterial>()) as BufferAddress;

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
struct ShaderMaterial {
    albedo: Vec4,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    anisotropy: f32,

    albedo_tex: Option<NonZeroU32>,
    normal_tex: Option<NonZeroU32>,
    roughness_tex: Option<NonZeroU32>,
    metallic_tex: Option<NonZeroU32>,
    reflectance_tex: Option<NonZeroU32>,
    clear_coat_tex: Option<NonZeroU32>,
    clear_coat_roughness_tex: Option<NonZeroU32>,
    anisotropy_tex: Option<NonZeroU32>,
    albedo_flags: AlbedoFlags,
}

unsafe impl bytemuck::Zeroable for ShaderMaterial {}
unsafe impl bytemuck::Pod for ShaderMaterial {}

pub struct MaterialManager {
    buffer: AutomatedBuffer,

    bind_group_cache: BindGroupCache<BufferCache1>,

    registry: ResourceRegistry<Material>,
}

impl MaterialManager {
    pub fn new(device: &Device, manager: &mut AutomatedBufferManager) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Material Manager");

        let buffer = manager.create_new_buffer(
            device,
            MAX_UNIFORM_BUFFER_BINDING_SIZE,
            BufferUsage::UNIFORM,
            Some("material buffer"),
        );
        let bind_group_cache = BindGroupCache::new();
        let registry = ResourceRegistry::new();

        Self {
            buffer,
            bind_group_cache,
            registry,
        }
    }

    pub fn allocate(&self) -> MaterialHandle {
        MaterialHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: MaterialHandle, material: Material) {
        span_transfer!(_ -> fill_span, INFO, "Material Manager Fill");

        self.registry.insert(handle.0, material);
    }

    pub fn remove(&mut self, handle: MaterialHandle) {
        self.registry.remove(handle.0);
    }

    pub fn internal_index(&self, handle: MaterialHandle) -> usize {
        self.registry.get_index_of(handle.0)
    }

    pub fn ready(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        texture_manager: &TextureManager,
        material_bgl: &BindGroupLayout,
    ) -> BufferCache1 {
        span_transfer!(_ -> ready_span, INFO, "Material Manager Ready");

        let registry = &self.registry;
        self.buffer
            .write_to_buffer(device, encoder, MATERIALS_SIZE, move |_, slice| {
                let typed_slice: &mut [ShaderMaterial] = bytemuck::cast_slice_mut(slice);

                let translate_texture = |v: TextureHandle| unsafe {
                    NonZeroU32::new_unchecked(texture_manager.internal_index(v) as u32 + 1)
                };

                for (index, material) in registry.values().enumerate() {
                    typed_slice[index] = ShaderMaterial {
                        albedo: material.albedo.to_value(),
                        roughness: material.roughness.to_value(0.0),
                        metallic: material.metallic.to_value(0.0),
                        reflectance: material.reflectance.to_value(0.5),
                        clear_coat: material.clear_coat.to_value(0.0),
                        clear_coat_roughness: material.clear_coat.to_value(0.0),
                        anisotropy: material.anisotropy.to_value(0.0),
                        albedo_tex: material.albedo.to_texture(translate_texture),
                        normal_tex: material.normal.map(translate_texture),
                        roughness_tex: material.reflectance.to_texture(translate_texture),
                        metallic_tex: material.metallic.to_texture(translate_texture),
                        reflectance_tex: material.reflectance.to_texture(translate_texture),
                        clear_coat_tex: material.clear_coat.to_texture(translate_texture),
                        clear_coat_roughness_tex: material.clear_coat_roughness.to_texture(translate_texture),
                        anisotropy_tex: material.anisotropy.to_texture(translate_texture),
                        albedo_flags: material.albedo.to_flags(),
                    }
                }
            });

        self.bind_group_cache.create_bind_group(&self.buffer, true, |buffer| {
            device.create_bind_group(&BindGroupDescriptor {
                label: Some("material bind group"),
                layout: &material_bgl,
                entries: &[BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Buffer(buffer.inner.slice(..)),
                }],
            })
        })
    }

    pub fn bind_group(&self, key: &BufferCache1) -> &BindGroup {
        self.bind_group_cache.get(key).unwrap()
    }
}
