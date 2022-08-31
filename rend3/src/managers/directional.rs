use crate::{
    managers::CameraManager,
    types::{Camera, CameraProjection, DirectionalLight, DirectionalLightHandle},
    util::{
        bind_merge::BindGroupBuilder, freelist::FreelistDerivedBuffer, scatter_copy::ScatterCopy, typedefs::FastHashMap,
    },
    INTERNAL_SHADOW_DEPTH_FORMAT, SHADOW_DIMENSIONS,
};
use arrayvec::ArrayVec;
use encase::{ArrayLength, ShaderType};
use glam::{Mat4, UVec2, Vec2, Vec3, Vec3A};
use rend3_types::{DirectionalLightChange, Handedness, RawDirectionalLightHandle};
use std::{
    mem::{self, size_of},
    num::{NonZeroU32, NonZeroU64},
};
use wgpu::{
    BindGroup, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Buffer,
    BufferBindingType, CommandEncoder, Device, Extent3d, ShaderStages, TextureAspect, TextureDescriptor,
    TextureDimension, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
};

mod shadow_alloc;

pub use shadow_alloc::ShadowCoordinate;

/// Internal representation of a directional light.
pub struct InternalDirectionalLight {
    pub inner: DirectionalLight,
}

#[derive(Debug, Clone, ShaderType)]
struct ShaderDirectionalLightBuffer {
    count: ArrayLength,
    #[size(runtime)]
    array: Vec<ShaderDirectionalLight>,
}

#[derive(Debug, Copy, Clone, ShaderType)]
struct ShaderDirectionalLight {
    pub view_proj: Mat4,
    pub color: Vec3,
    pub direction: Vec3,
    pub offset: Vec2,
    pub size: f32,
}

/// Manages directional lights and their associated shadow maps.
pub struct DirectionalLightManager {
    data: Vec<Option<InternalDirectionalLight>>,
}
impl DirectionalLightManager {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("DirectionalLightManager::new");

        Self { data: Vec::new() }
    }

    pub fn add(&mut self, handle: &DirectionalLightHandle, light: DirectionalLight) {
        if handle.idx > self.data.len() {
            self.data.resize_with(handle.idx + 1, || None);
        }
        self.data[handle.idx] = Some(InternalDirectionalLight { inner: light })
    }

    pub fn update(&mut self, handle: RawDirectionalLightHandle, change: DirectionalLightChange) {
        self.data[handle.idx]
            .as_mut()
            .unwrap()
            .inner
            .update_from_changes(change);
    }

    pub fn remove(&mut self, handle: RawDirectionalLightHandle) {
        self.data[handle.idx].take().unwrap();
    }

    pub fn ready(&mut self, device: &Device, user_camera: &CameraManager) -> Vec<CameraManager> {
        profiling::scope!("Directional Light Ready");

        todo!()
    }
}

fn create_shadow_texture(device: &Device, size: Extent3d) -> (TextureView, Vec<TextureView>) {
    profiling::scope!("shadow texture creation");

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("shadow texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: INTERNAL_SHADOW_DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
    });

    let primary_view = texture.create_view(&TextureViewDescriptor {
        label: Some("shadow texture view"),
        format: None,
        dimension: Some(TextureViewDimension::D2Array),
        aspect: TextureAspect::All,
        base_mip_level: 0,
        mip_level_count: None,
        base_array_layer: 0,
        array_layer_count: None,
    });

    let layer_views: Vec<_> = (0..size.depth_or_array_layers)
        .map(|idx| {
            texture.create_view(&TextureViewDescriptor {
                label: Some(&format!("shadow texture layer {}", idx)),
                format: None,
                dimension: Some(TextureViewDimension::D2),
                aspect: TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: None,
                base_array_layer: idx,
                array_layer_count: NonZeroU32::new(1),
            })
        })
        .collect();

    (primary_view, layer_views)
}
