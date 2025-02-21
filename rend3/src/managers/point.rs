use encase::{ArrayLength, ShaderType};
use glam::{Vec3, Vec4};
use rend3_types::{PointLight, PointLightChange, RawPointLightHandle};
use wgpu::{BufferUsages, Device, ShaderStages};

use crate::{
    util::{
        bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
        buffer::WrappedPotBuffer,
    },
    Renderer,
};

#[derive(Debug, Clone, ShaderType)]
struct ShaderPointLightBuffer {
    count: ArrayLength,
    #[size(runtime)]
    array: Vec<ShaderPointLight>,
}

#[derive(Debug, Copy, Clone, ShaderType)]
struct ShaderPointLight {
    pub position: Vec4,
    pub color: Vec3,
    pub radius: f32,
}

/// Manages point lights and their associated shadow maps.
pub struct PointLightManager {
    data: Vec<Option<PointLight>>,
    data_buffer: WrappedPotBuffer<ShaderPointLightBuffer>,
}

impl PointLightManager {
    pub fn new(device: &Device) -> Self {
        Self {
            data: Vec::new(),
            data_buffer: WrappedPotBuffer::new(device, BufferUsages::STORAGE, "point light buffer"),
        }
    }

    pub fn add(&mut self, handle: &RawPointLightHandle, light: PointLight) {
        if handle.idx >= self.data.len() {
            self.data.resize(handle.idx + 1, None);
        }

        self.data[handle.idx] = Some(light);
    }

    pub fn update(&mut self, handle: RawPointLightHandle, change: PointLightChange) {
        self.data[handle.idx].as_mut().unwrap().update_from_changes(change);
    }

    pub fn remove(&mut self, handle: &RawPointLightHandle) {
        self.data[handle.idx].take().unwrap();
    }

    pub fn evaluate(&mut self, renderer: &Renderer) {
        let buffer = ShaderPointLightBuffer {
            count: ArrayLength,
            array: self
                .data
                .iter()
                .flatten()
                .map(|light| ShaderPointLight {
                    position: light.position.extend(1.0),
                    color: light.color * light.intensity,
                    radius: light.radius,
                })
                .collect(),
        };

        self.data_buffer.write_to_buffer(&renderer.device, &renderer.queue, &buffer);
    }

    pub fn add_to_bgl(bglb: &mut BindGroupLayoutBuilder) {
        bglb.append(
            ShaderStages::FRAGMENT,
            wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: Some(ShaderPointLightBuffer::min_size()),
            },
            None,
        );
    }

    pub fn add_to_bg<'a>(&'a self, bgb: &mut BindGroupBuilder<'a>) {
        bgb.append_buffer(&self.data_buffer);
    }
}
