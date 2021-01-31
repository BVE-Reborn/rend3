use crate::renderer::{camera::CameraManager, frustum::ShaderFrustum};
use glam::Mat4;
use std::mem::size_of;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, Buffer, BufferAddress, BufferDescriptor,
    BufferUsage, Device, Queue,
};

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
pub struct ShaderCommonUniform {
    view: Mat4,
    view_proj: Mat4,
    inv_view: Mat4,
    inv_origin_view_proj: Mat4,
    frustum: ShaderFrustum,
}

unsafe impl bytemuck::Zeroable for ShaderCommonUniform {}
unsafe impl bytemuck::Pod for ShaderCommonUniform {}

pub struct WrappedUniform {
    buffer: Buffer,
    pub uniform_bg: BindGroup,
}
impl WrappedUniform {
    pub fn new(device: &Device, uniform_bgl: &BindGroupLayout) -> Self {
        span_transfer!(_ -> new_span, WARN, "Creating WrappedUniform");

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
                resource: buffer.as_entire_binding(),
            }],
        });

        Self { buffer, uniform_bg }
    }

    pub fn upload<'a>(&'a self, queue: &Queue, camera: &CameraManager) {
        span_transfer!(_ -> upload_span, WARN, "Uploading WrappedUniform");

        let view = camera.view();

        let uniforms = ShaderCommonUniform {
            view,
            view_proj: camera.view_proj(),
            inv_view: view.inverse(),
            inv_origin_view_proj: camera.origin_view_proj().inverse(),
            frustum: ShaderFrustum::from_matrix(camera.proj()),
        };

        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&uniforms));
    }
}
