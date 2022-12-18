//! Helpers for building the per-camera uniform data used for cameras and
//! shadows.

use std::iter::once;

use glam::{Mat4, UVec2, Vec4};
use rend3::{
    graph::{DataHandle, RenderGraph, RenderTargetHandle},
    managers::{CameraManager},
    util::{bind_merge::BindGroupBuilder, frustum::ShaderFrustum},
};
use wgpu::{
    util::{BufferInitDescriptor, DeviceExt},
    BindGroup, BufferUsages,
};

use crate::common::{Samplers, WholeFrameInterfaces};

/// The actual structure passed to the shader.
#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
pub struct FrameUniforms {
    pub view: Mat4,
    pub view_proj: Mat4,
    pub origin_view_proj: Mat4,
    pub inv_view: Mat4,
    pub inv_view_proj: Mat4,
    pub inv_origin_view_proj: Mat4,
    pub frustum: ShaderFrustum,
    pub ambient: Vec4,
    pub resolution: UVec2,
}
impl FrameUniforms {
    /// Use the given camera to generate these uniforms.
    pub fn new(camera: &CameraManager, ambient: Vec4, resolution: UVec2) -> Self {
        profiling::scope!("create uniforms");

        let view = camera.view();
        let view_proj = camera.view_proj();
        let origin_view_proj = camera.origin_view_proj();

        Self {
            view,
            view_proj,
            origin_view_proj,
            inv_view: view.inverse(),
            inv_view_proj: view_proj.inverse(),
            inv_origin_view_proj: origin_view_proj.inverse(),
            frustum: ShaderFrustum::from_matrix(camera.proj()),
            ambient,
            resolution,
        }
    }
}

unsafe impl bytemuck::Zeroable for FrameUniforms {}
unsafe impl bytemuck::Pod for FrameUniforms {}

/// Add the creation of these uniforms to the graph.
pub fn add_to_graph<'node>(
    graph: &mut RenderGraph<'node>,
    shadow_uniform_bg: DataHandle<BindGroup>,
    forward_uniform_bg: DataHandle<BindGroup>,
    shadow_target: RenderTargetHandle,
    interfaces: &'node WholeFrameInterfaces,
    samplers: &'node Samplers,
    ambient: Vec4,
    resolution: UVec2,
) {
    let mut builder = graph.add_node("build uniform data");
    let shadow_handle = builder.add_data_output(shadow_uniform_bg);
    let forward_handle = builder.add_data_output(forward_uniform_bg);

    // Get the shadow target and declare it a dependency of the forward_uniform_bg
    let shadow_target_handle = builder.add_render_target_output(shadow_target);
    builder.add_dependencies_to_render_targets(forward_uniform_bg, once(shadow_target));

    builder.build(move |_pt, renderer, _encoder_or_pass, _temps, _ready, graph_data| {
        let shadow_target = graph_data.get_render_target(shadow_target_handle);

        let mut bgb = BindGroupBuilder::new();

        samplers.add_to_bg(&mut bgb);

        let uniforms = FrameUniforms::new(graph_data.camera_manager, ambient, resolution);
        let uniform_buffer = renderer.device.create_buffer_init(&BufferInitDescriptor {
            label: Some("frame uniform"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: BufferUsages::UNIFORM,
        });

        bgb.append_buffer(&uniform_buffer);

        graph_data.directional_light_manager.add_to_bg(&mut bgb);

        let shadow_uniform_bg = bgb.build(
            &renderer.device,
            Some("shadow uniform bg"),
            &interfaces.depth_uniform_bgl,
        );

        bgb.append_texture_view(shadow_target);

        let forward_uniform_bg = bgb.build(
            &renderer.device,
            Some("forward uniform bg"),
            &interfaces.forward_uniform_bgl,
        );

        graph_data.set_data(shadow_handle, Some(shadow_uniform_bg));
        graph_data.set_data(forward_handle, Some(forward_uniform_bg));
    })
}
