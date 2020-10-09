use crate::{
    bind_merge::BindGroupBuilder,
    datatypes::{DirectionalLight, DirectionalLightHandle, TextureHandle},
    registry::ResourceRegistry,
    renderer::{camera::Camera, passes, passes::ShadowPassSet, texture::TextureManager, INTERNAL_SHADOW_DEPTH_FORMAT},
};
use glam::{Mat4, Vec3};
use std::{mem::size_of, num::NonZeroU32, sync::Arc};
use wgpu::{
    BindGroupEntry, BindGroupLayout, BindingResource, BufferAddress, BufferUsage, CommandEncoder, Device, Extent3d,
    TextureDescriptor, TextureDimension, TextureUsage, TextureViewDescriptor,
};
use wgpu_conveyor::{write_to_buffer1, AutomatedBuffer, AutomatedBufferManager, IdBuffer};

pub struct InternalDirectionalLight {
    pub inner: DirectionalLight,
    pub camera: Camera,
    pub shadow_tex: Option<TextureHandle>,
    pub shadow_pass_set: passes::ShadowPassSet,
}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
struct ShaderDirectionalLightBufferHeader {
    total_lights: u32,
}

unsafe impl bytemuck::Zeroable for ShaderDirectionalLightBufferHeader {}
unsafe impl bytemuck::Pod for ShaderDirectionalLightBufferHeader {}

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
struct ShaderDirectionalLight {
    pub inv_view_proj: Mat4,
    pub color: Vec3,
    pub shadow_tex: Option<NonZeroU32>,
    pub direction: Vec3,
}

unsafe impl bytemuck::Zeroable for ShaderDirectionalLight {}
unsafe impl bytemuck::Pod for ShaderDirectionalLight {}

pub struct DirectionalLightManager {
    buffer_storage: Option<Arc<IdBuffer>>,
    buffer: AutomatedBuffer,

    registry: ResourceRegistry<InternalDirectionalLight>,
}
impl DirectionalLightManager {
    pub fn new(device: &Device, buffer_manager: &mut AutomatedBufferManager) -> Self {
        let registry = ResourceRegistry::new();

        let buffer = buffer_manager.create_new_buffer(device, 0, BufferUsage::STORAGE, Some("directional lights"));

        Self {
            buffer_storage: None,
            buffer,
            registry,
        }
    }

    pub fn allocate(&self) -> DirectionalLightHandle {
        DirectionalLightHandle(self.registry.allocate())
    }

    pub fn fill(
        &mut self,
        device: &Device,
        texture_manager_internal: &mut TextureManager,
        uniform_bgl: &BindGroupLayout,
        handle: DirectionalLightHandle,
        light: DirectionalLight,
    ) {
        let texture_handle = texture_manager_internal.allocate();

        let texture = device.create_texture(&TextureDescriptor {
            // TODO: label
            label: None,
            // TODO: shadow map sizes
            size: Extent3d {
                width: 2048,
                height: 2048,
                depth: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: INTERNAL_SHADOW_DEPTH_FORMAT,
            usage: TextureUsage::OUTPUT_ATTACHMENT | TextureUsage::SAMPLED,
        });
        let view = texture.create_view(&TextureViewDescriptor::default());

        texture_manager_internal.fill(texture_handle, view, None);

        let shadow_pass_set = ShadowPassSet::new(device, uniform_bgl, String::from("directional light"));

        self.registry.insert(
            handle.0,
            InternalDirectionalLight {
                inner: light,
                camera: Camera::new_orthographic(light.direction),
                shadow_tex: Some(texture_handle),
                shadow_pass_set,
            },
        );
    }

    pub fn get_mut(&mut self, handle: DirectionalLightHandle) -> &mut InternalDirectionalLight {
        self.registry.get_mut(handle.0)
    }

    pub fn remove(&mut self, handle: DirectionalLightHandle) {
        self.registry.remove(handle.0);
    }

    pub fn ready(&mut self, device: &Device, encoder: &mut CommandEncoder, texture_manager: &TextureManager) {
        let translate_texture = texture_manager.translation_fn();

        let registry = &self.registry;

        let size = self.registry.count() * size_of::<ShaderDirectionalLight>()
            + size_of::<ShaderDirectionalLightBufferHeader>();
        write_to_buffer1(
            device,
            encoder,
            &mut self.buffer,
            size as BufferAddress,
            |_, raw_buffer| {
                let (raw_buffer_header, raw_buffer_body) =
                    raw_buffer.split_at_mut(size_of::<ShaderDirectionalLightBufferHeader>());
                let buffer_header: &mut ShaderDirectionalLightBufferHeader =
                    bytemuck::from_bytes_mut(raw_buffer_header);
                let buffer_body: &mut [ShaderDirectionalLight] = bytemuck::cast_slice_mut(raw_buffer_body);

                buffer_header.total_lights = registry.count() as u32;

                for (idx, light) in registry.values().enumerate() {
                    buffer_body[idx] = ShaderDirectionalLight {
                        inv_view_proj: light.camera.view_proj().inverse(),
                        color: light.inner.color * light.inner.intensity,
                        direction: light.inner.direction,
                        shadow_tex: light.shadow_tex.map(translate_texture),
                    }
                }
            },
        );

        self.buffer_storage = Some(self.buffer.get_current_inner());
    }

    pub fn append_to_bgb<'a>(&'a self, builder: &mut BindGroupBuilder<'a>) {
        builder.append(BindGroupEntry {
            binding: 0,
            resource: BindingResource::Buffer(self.buffer_storage.as_ref().unwrap().inner.slice(..)),
        })
    }

    pub fn values(&self) -> impl Iterator<Item = &InternalDirectionalLight> {
        self.registry.values()
    }
}
