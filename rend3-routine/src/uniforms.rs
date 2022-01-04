use glam::{Mat4, Vec4};
use rend3::{
    managers::CameraManager,
    util::{bind_merge::BindGroupBuilder, frustum::ShaderFrustum},
    DataHandle, RenderGraph,
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, Buffer, BufferUsages, Device,
};

use crate::common::{GenericShaderInterfaces, Samplers};

#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
pub struct ShaderCommonUniform {
    view: Mat4,
    view_proj: Mat4,
    origin_view_proj: Mat4,
    inv_view: Mat4,
    inv_view_proj: Mat4,
    inv_origin_view_proj: Mat4,
    frustum: ShaderFrustum,
    ambient: Vec4,
}

unsafe impl bytemuck::Zeroable for ShaderCommonUniform {}
unsafe impl bytemuck::Pod for ShaderCommonUniform {}

pub fn create_shader_uniform(device: &Device, camera: &CameraManager, ambient: Vec4) -> Buffer {
    profiling::scope!("create uniforms");

    let view = camera.view();
    let view_proj = camera.view_proj();
    let origin_view_proj = camera.origin_view_proj();

    let uniforms = ShaderCommonUniform {
        view,
        view_proj,
        origin_view_proj,
        inv_view: view.inverse(),
        inv_view_proj: view_proj.inverse(),
        inv_origin_view_proj: origin_view_proj.inverse(),
        frustum: ShaderFrustum::from_matrix(camera.proj()),
        ambient,
    };

    device.create_buffer_init(&BufferInitDescriptor {
        label: Some("shader uniform"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: BufferUsages::UNIFORM,
    })
}

pub fn add_to_graph<'node>(
    graph: &mut RenderGraph<'node>,
    shadow_uniform_bg: DataHandle<BindGroup>,
    forward_uniform_bg: DataHandle<BindGroup>,
    interfaces: &'node GenericShaderInterfaces,
    samplers: &'node Samplers,
    ambient: Vec4,
) {
    let mut builder = graph.add_node("build uniform data");
    let shadow_handle = builder.add_data_output(shadow_uniform_bg);
    let forward_handle = builder.add_data_output(forward_uniform_bg);
    builder.build(move |_pt, renderer, _encoder_or_pass, _temps, _ready, graph_data| {
        let mut bgb = BindGroupBuilder::new();

        samplers.add_to_bg(&mut bgb);

        let uniform_buffer = create_shader_uniform(&renderer.device, graph_data.camera_manager, ambient);

        bgb.append_buffer(&uniform_buffer);

        let shadow_uniform_bg = bgb.build(
            &renderer.device,
            Some("shadow uniform bg"),
            &interfaces.depth_uniform_bgl,
        );

        graph_data.directional_light_manager.add_to_bg(&mut bgb);

        let forward_uniform_bg = bgb.build(
            &renderer.device,
            Some("forward uniform bg"),
            &interfaces.forward_uniform_bgl,
        );

        graph_data.set_data(shadow_handle, Some(shadow_uniform_bg));
        graph_data.set_data(forward_handle, Some(forward_uniform_bg));
    })
}
