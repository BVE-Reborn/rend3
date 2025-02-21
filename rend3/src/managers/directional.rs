use encase::{ArrayLength, ShaderType};
use glam::{Mat4, UVec2, Vec2, Vec3};
use rend3_types::{DirectionalLightChange, RawDirectionalLightHandle};
use wgpu::{
    BindingType, BufferBindingType, BufferUsages, Device, Extent3d, ShaderStages, TextureDescriptor, TextureDimension,
    TextureUsages, TextureView, TextureViewDescriptor,
};

use crate::{
    managers::CameraState,
    types::DirectionalLight,
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        buffer::WrappedPotBuffer,
    },
    Renderer, INTERNAL_SHADOW_DEPTH_FORMAT,
};

mod shadow_alloc;
mod shadow_camera;

pub use shadow_alloc::ShadowMap;

const MINIMUM_SHADOW_MAP_SIZE: UVec2 = UVec2::splat(32);

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
    /// View/Projection of directional light. Shadow rendering uses viewports
    /// so this always outputs [-1, 1] no matter where in the atlast the shadow is.
    pub view_proj: Mat4,
    /// Color/intensity of the light
    pub color: Vec3,
    /// Direction of the light
    pub direction: Vec3,
    /// 1 / resolution of whole shadow map
    pub inv_resolution: Vec2,
    /// [0, 1] offset of the shadow map in the atlas.
    pub atlas_offset: Vec2,
    /// [0, 1] size of the shadow map in the atlas.
    pub atlas_size: Vec2,
}

#[derive(Debug, Clone)]
pub struct ShadowDesc {
    pub map: ShadowMap,
    pub camera: CameraState,
}

/// Manages directional lights and their associated shadow maps.
pub struct DirectionalLightManager {
    data: Vec<Option<InternalDirectionalLight>>,
    data_buffer: WrappedPotBuffer<ShaderDirectionalLightBuffer>,

    texture_size: UVec2,
    texture_view: TextureView,
}
impl DirectionalLightManager {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("DirectionalLightManager::new");

        let texture_size = MINIMUM_SHADOW_MAP_SIZE;
        let texture_view = create_shadow_texture(device, texture_size);

        Self {
            data: Vec::new(),
            data_buffer: WrappedPotBuffer::new(device, BufferUsages::STORAGE, "shadow data buffer"),
            texture_size,
            texture_view,
        }
    }

    pub fn add(&mut self, handle: &RawDirectionalLightHandle, light: DirectionalLight) {
        if handle.idx >= self.data.len() {
            self.data.resize_with(handle.idx + 1, || None);
        }
        self.data[handle.idx] = Some(InternalDirectionalLight { inner: light })
    }

    pub fn update(&mut self, handle: &RawDirectionalLightHandle, change: DirectionalLightChange) {
        self.data[handle.idx].as_mut().unwrap().inner.update_from_changes(change);
    }

    pub fn remove(&mut self, handle: &RawDirectionalLightHandle) {
        self.data[handle.idx].take().unwrap();
    }

    pub fn evaluate(&mut self, renderer: &Renderer, user_camera: &CameraState) -> (UVec2, Vec<ShadowDesc>) {
        profiling::scope!("DirectionalLightManager::evaluate");

        let shadow_maps: Vec<_> = self
            .data
            .iter()
            .enumerate()
            .filter_map(|(idx, light)| Some((RawDirectionalLightHandle::new(idx), light.as_ref()?.inner.resolution)))
            .collect();
        let shadow_atlas = shadow_alloc::allocate_shadow_atlas(shadow_maps, renderer.limits.max_texture_dimension_2d);

        let new_shadow_map_size = match shadow_atlas {
            Some(ref m) => m.texture_dimensions.max(MINIMUM_SHADOW_MAP_SIZE),
            None => MINIMUM_SHADOW_MAP_SIZE,
        };
        let new_shadow_map_size_f32 = new_shadow_map_size.as_vec2();

        if new_shadow_map_size != self.texture_size {
            self.texture_size = new_shadow_map_size;
            self.texture_view = create_shadow_texture(&renderer.device, self.texture_size);
        }

        let coordinates = match shadow_atlas {
            Some(m) => m.maps,
            None => return (new_shadow_map_size, Vec::new()),
        };

        let shadow_data: Vec<_> = coordinates
            .into_iter()
            .map(|map| {
                let camera = shadow_camera::shadow_camera(self.data[map.handle.idx].as_ref().unwrap(), user_camera);

                ShadowDesc { map, camera }
            })
            .collect();

        let buffer = ShaderDirectionalLightBuffer {
            count: ArrayLength,
            array: shadow_data
                .iter()
                .map(|desc| {
                    let light = &self.data[desc.map.handle.idx].as_ref().unwrap().inner;

                    ShaderDirectionalLight {
                        view_proj: desc.camera.view_proj(),
                        color: light.color * light.intensity,
                        direction: light.direction,
                        inv_resolution: 1.0 / new_shadow_map_size_f32,
                        atlas_offset: desc.map.offset.as_vec2() / new_shadow_map_size_f32,
                        atlas_size: desc.map.size as f32 / new_shadow_map_size_f32,
                    }
                })
                .collect(),
        };

        self.data_buffer.write_to_buffer(&renderer.device, &renderer.queue, &buffer);

        (new_shadow_map_size, shadow_data)
    }

    pub fn add_to_bgl(bglb: &mut BindGroupLayoutBuilder) {
        bglb.append(
            ShaderStages::VERTEX_FRAGMENT,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: Some(ShaderDirectionalLightBuffer::min_size()),
            },
            None,
        );
    }

    pub fn add_to_bg<'a>(&'a self, bgb: &mut BindGroupBuilder<'a>) {
        bgb.append_buffer(&self.data_buffer);
    }
}

fn create_shadow_texture(device: &Device, size: UVec2) -> TextureView {
    profiling::scope!("shadow texture creation");

    let texture = device.create_texture(&TextureDescriptor {
        label: Some("rend3 shadow texture"),
        size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: INTERNAL_SHADOW_DEPTH_FORMAT,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });

    texture.create_view(&TextureViewDescriptor {
        label: Some("rend3 shadow texture view"),
        ..TextureViewDescriptor::default()
    })
}
