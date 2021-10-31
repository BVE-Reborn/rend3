use glam::{Mat4, Vec4};
use rend3::{
    managers::CameraManager,
    util::{bind_merge::BindGroupBuilder, frustum::ShaderFrustum},
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BufferUsages, Device,
};

use crate::common::interfaces::ShaderInterfaces;

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

pub struct CreateShaderUniformArgs<'a> {
    pub device: &'a Device,
    pub camera: &'a CameraManager,

    pub interfaces: &'a ShaderInterfaces,

    pub ambient: Vec4,
}

pub fn create_shader_uniform(args: CreateShaderUniformArgs<'_>) -> BindGroup {
    profiling::scope!("create uniforms");

    let view = args.camera.view();

    let uniforms = ShaderCommonUniform {
        view,
        view_proj: args.camera.view_proj(),
        inv_view: view.inverse(),
        inv_origin_view_proj: args.camera.origin_view_proj().inverse(),
        frustum: ShaderFrustum::from_matrix(args.camera.proj()),
        ambient: args.ambient,
    };

    let buffer = args.device.create_buffer_init(&BufferInitDescriptor {
        label: Some("shader uniform"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: BufferUsages::UNIFORM,
    });

    BindGroupBuilder::new(Some("shader uniform"))
        .append_buffer(&buffer)
        .build(args.device, &args.interfaces.uniform_bgl)
}
