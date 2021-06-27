use crate::{
    datatypes::{Camera, CameraProjection, DirectionalLight, DirectionalLightHandle},
    modules::CameraManager,
    util::{bind_merge::BindGroupBuilder, buffer::WrappedPotBuffer, registry::ResourceRegistry},
    INTERNAL_SHADOW_DEPTH_FORMAT, SHADOW_DIMENSIONS,
};
use glam::{Mat4, Vec3};
use std::{mem::size_of, num::NonZeroU32, sync::Arc};
use wgpu::{
    BindingResource, BindingType, BufferBindingType, BufferUsage, Device, Extent3d, Queue, ShaderStage, TextureAspect,
    TextureDescriptor, TextureDimension, TextureSampleType, TextureUsage, TextureView, TextureViewDescriptor,
    TextureViewDimension,
};

pub struct InternalDirectionalLight {
    pub inner: DirectionalLight,
    pub camera: CameraManager,
    pub shadow_tex: u32,
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
    pub view_proj: Mat4,
    pub color: Vec3,
    pub shadow_tex: u32,
    pub direction: Vec3,
}

unsafe impl bytemuck::Zeroable for ShaderDirectionalLight {}
unsafe impl bytemuck::Pod for ShaderDirectionalLight {}

pub struct DirectionalLightManager {
    buffer: WrappedPotBuffer,

    view: TextureView,
    layer_views: Vec<Arc<TextureView>>,

    registry: ResourceRegistry<InternalDirectionalLight>,
}
impl DirectionalLightManager {
    pub fn new(device: &Device) -> Self {
        let registry = ResourceRegistry::new();

        let buffer = WrappedPotBuffer::new(device, 0, BufferUsage::STORAGE, Some("directional lights"));

        let (view, layer_views) = create_shadow_texture(device, 1);

        Self {
            buffer,
            view,
            layer_views,
            registry,
        }
    }

    pub fn allocate(&self) -> DirectionalLightHandle {
        DirectionalLightHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: DirectionalLightHandle, light: DirectionalLight) {
        self.registry.insert(
            handle.0,
            InternalDirectionalLight {
                inner: light,
                camera: CameraManager::new(
                    Camera {
                        projection: CameraProjection::from_orthographic_direction(light.direction.into()),
                        ..Camera::default()
                    },
                    None,
                ),
                shadow_tex: self.registry.count() as u32,
            },
        );
    }

    pub fn get_mut(&mut self, handle: DirectionalLightHandle) -> &mut InternalDirectionalLight {
        self.registry.get_mut(handle.0)
    }

    pub fn get_layer_view_arc(&self, layer: u32) -> Arc<TextureView> {
        Arc::clone(&self.layer_views[layer as usize])
    }

    pub fn remove(&mut self, handle: DirectionalLightHandle) {
        self.registry.remove(handle.0);
    }

    pub fn ready(&mut self, device: &Device, queue: &Queue) {
        let registered_count = self.registry.count();
        if registered_count != self.layer_views.len() && registered_count != 0 {
            let (view, layer_views) = create_shadow_texture(device, registered_count as u32);
            self.view = view;
            self.layer_views = layer_views;
        }

        let registry = &self.registry;

        let size = self.registry.count() * size_of::<ShaderDirectionalLight>()
            + size_of::<ShaderDirectionalLightBufferHeader>();

        let mut buffer = Vec::with_capacity(size);
        buffer.extend_from_slice(bytemuck::bytes_of(&ShaderDirectionalLightBufferHeader {
            total_lights: registry.count() as u32,
        }));
        for light in registry.values() {
            buffer.extend_from_slice(bytemuck::bytes_of(&ShaderDirectionalLight {
                view_proj: light.camera.view_proj(),
                color: light.inner.color * light.inner.intensity,
                direction: light.inner.direction,
                shadow_tex: light.shadow_tex as u32,
            }));
        }

        self.buffer.write_to_buffer(device, queue, &buffer);
    }

    pub fn append_to_bgb<'a>(&'a self, visibility: ShaderStage, builder: &mut BindGroupBuilder<'a>) {
        builder.append(
            visibility,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            None,
            self.buffer.as_entire_binding(),
        );
        builder.append(
            visibility,
            BindingType::Texture {
                view_dimension: TextureViewDimension::D2Array,
                sample_type: TextureSampleType::Float { filterable: true },
                multisampled: false,
            },
            None,
            BindingResource::TextureView(&self.view),
        );
    }

    pub fn values(&self) -> impl Iterator<Item = &InternalDirectionalLight> {
        self.registry.values()
    }
}

fn create_shadow_texture(device: &Device, count: u32) -> (TextureView, Vec<Arc<TextureView>>) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("shadow texture"),
        size: Extent3d {
            width: SHADOW_DIMENSIONS,
            height: SHADOW_DIMENSIONS,
            depth: count,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: INTERNAL_SHADOW_DEPTH_FORMAT,
        usage: TextureUsage::RENDER_ATTACHMENT | TextureUsage::SAMPLED,
    });

    let primary_view = texture.create_view(&TextureViewDescriptor {
        label: Some("shadow texture view"),
        format: None,
        dimension: Some(TextureViewDimension::D2Array),
        aspect: TextureAspect::All,
        base_mip_level: 0,
        level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
    });

    let layer_views: Vec<_> = (0..count)
        .map(|idx| {
            Arc::new(texture.create_view(&TextureViewDescriptor {
                label: Some(&format!("shadow texture layer {}", count)),
                format: None,
                dimension: Some(TextureViewDimension::D2),
                aspect: TextureAspect::All,
                base_mip_level: 0,
                level_count: None,
                base_array_layer: idx,
                array_layer_count: NonZeroU32::new(1),
            }))
        })
        .collect();

    (primary_view, layer_views)
}
