use crate::renderer::camera::Camera;
use glam::Mat4;
use std::mem::size_of;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Buffer, BufferAddress,
    BufferDescriptor, BufferUsage, Device, Queue,
};

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
pub struct ShaderCommonUniform {
    view: Mat4,
    view_proj: Mat4,
}

unsafe impl bytemuck::Zeroable for ShaderCommonUniform {}
unsafe impl bytemuck::Pod for ShaderCommonUniform {}

pub struct UniformManager {
    buffer: Buffer,
    pub uniform_bg: BindGroup,
}
impl UniformManager {
    pub fn new(device: &Device, uniform_bgl: &BindGroupLayout) -> Self {
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("uniform buffer"),
            size: size_of::<ShaderCommonUniform>() as BufferAddress,
            usage: BufferUsage::COPY_DST | BufferUsage::UNIFORM,
            mapped_at_creation: false,
        });

        let uniform_bg = device.create_bind_group(&BindGroupDescriptor {
            label: Some("uniform bg"),
            layout: uniform_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: BindingResource::Buffer(buffer.slice(..)),
            }],
        });

        Self { buffer, uniform_bg }
    }

    pub fn upload(&self, queue: &Queue, camera: &Camera) {
        let uniforms = ShaderCommonUniform {
            view: camera.view(),
            view_proj: camera.view_proj(),
        };

        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&uniforms));
    }
}
