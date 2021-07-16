use crate::{
    cache::BindGroupCache,
    resources::CameraManager,
    util::{bind_merge::BindGroupBuilder, frustum::ShaderFrustum},
};
use glam::{Mat4, Vec4};
use std::{mem, num::NonZeroU64, sync::Arc};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BindGroupLayout, BindingType, BufferBindingType, BufferUsage, Device, ShaderStage,
};

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
pub struct ShaderCommonUniform {
    view: Mat4,
    view_proj: Mat4,
    inv_view: Mat4,
    inv_origin_view_proj: Mat4,
    frustum: ShaderFrustum,
    ambient: Vec4,
}

unsafe impl bytemuck::Zeroable for ShaderCommonUniform {}
unsafe impl bytemuck::Pod for ShaderCommonUniform {}

pub fn shader_uniform(
    device: &Device,
    visibility: ShaderStage,
    bind_group_cache: &mut BindGroupCache,
    camera: &CameraManager,
    ambient: Vec4,
) -> (Arc<BindGroupLayout>, Arc<BindGroup>) {
    let view = camera.view();

    let uniforms = ShaderCommonUniform {
        view,
        view_proj: camera.view_proj(),
        inv_view: view.inverse(),
        inv_origin_view_proj: camera.origin_view_proj().inverse(),
        frustum: ShaderFrustum::from_matrix(camera.proj()),
        ambient,
    };

    let buffer = device.create_buffer_init(&BufferInitDescriptor {
        label: Some("shader uniform"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: BufferUsage::UNIFORM,
    });

    let mut bgb = BindGroupBuilder::new("shader uniform");
    bgb.append(
        visibility,
        BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(mem::size_of::<ShaderCommonUniform>() as _),
        },
        None,
        buffer.as_entire_binding(),
    );
    bgb.build_transient(device, bind_group_cache)
}
