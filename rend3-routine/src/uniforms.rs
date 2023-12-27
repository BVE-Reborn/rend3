//! Helpers for building the per-camera uniform data used for cameras and
//! shadows.

use encase::{ShaderSize, ShaderType, UniformBuffer};
use glam::{Mat4, UVec2, Vec4};
use rend3::{
    graph::{DataHandle, NodeResourceUsage, RenderGraph, RenderTargetHandle},
    managers::CameraState,
    util::{bind_merge::BindGroupBuilder, frustum::Frustum},
};
use wgpu::{BindGroup, BufferUsages};

use crate::common::{Samplers, WholeFrameInterfaces};

/// Set of uniforms that are useful for the whole frame.
#[derive(Debug, Copy, Clone, ShaderType)]
pub struct FrameUniforms {
    pub view: Mat4,
    pub view_proj: Mat4,
    pub origin_view_proj: Mat4,
    pub inv_view: Mat4,
    pub inv_view_proj: Mat4,
    pub inv_origin_view_proj: Mat4,
    pub frustum: Frustum,
    pub ambient: Vec4,
    pub resolution: UVec2,
}
impl FrameUniforms {
    /// Use the given camera to generate these uniforms.
    pub fn new(camera: &CameraState, info: &UniformInformation<'_>) -> Self {
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
            frustum: Frustum::from_matrix(camera.proj()),
            ambient: info.ambient,
            resolution: info.resolution,
        }
    }
}

/// Various information sources for the uniform data.
pub struct UniformInformation<'node> {
    /// Struct containing the default set of samplers.
    pub samplers: &'node Samplers,
    /// Ambient light color.
    pub ambient: Vec4,
    /// Resolution of the viewport.
    pub resolution: UVec2,
}

pub struct UniformBindingHandles<'node> {
    /// Interfaces containing the bind group layouts for the uniform bind groups.
    pub interfaces: &'node WholeFrameInterfaces,
    /// The output bind group handle for the shadow uniform data. This does not
    /// include the shadow map texture, preventing a cycle.
    pub shadow_uniform_bg: DataHandle<BindGroup>,
    /// The output bind group handle for the forward uniform data. This does
    /// include the shadow map texture.
    pub forward_uniform_bg: DataHandle<BindGroup>,
}

/// Add the creation of these uniforms to the graph.
pub fn add_to_graph<'node>(
    graph: &mut RenderGraph<'node>,
    shadow_target: RenderTargetHandle,
    binding_handles: UniformBindingHandles<'node>,
    info: UniformInformation<'node>,
) {
    let mut builder = graph.add_node("build uniform data");
    let shadow_handle = builder.add_data(binding_handles.shadow_uniform_bg, NodeResourceUsage::Output);
    let forward_handle = builder.add_data(binding_handles.forward_uniform_bg, NodeResourceUsage::Output);

    // Get the shadow target and declare it a dependency of the forward_uniform_bg
    let shadow_target_handle = builder.add_render_target(shadow_target, NodeResourceUsage::Reference);
    builder.add_dependencies_to_render_targets(binding_handles.forward_uniform_bg, [shadow_target]);

    builder.build(move |ctx| {
        let shadow_target = ctx.graph_data.get_render_target(shadow_target_handle);

        let mut bgb = BindGroupBuilder::new();

        info.samplers.add_to_bg(&mut bgb);

        let uniforms = FrameUniforms::new(&ctx.data_core.viewport_camera_state, &info);
        let uniform_buffer = ctx.renderer.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Frame Uniforms"),
            size: FrameUniforms::SHADER_SIZE.get(),
            usage: BufferUsages::UNIFORM,
            mapped_at_creation: true,
        });
        let mut mapping = uniform_buffer.slice(..).get_mapped_range_mut();
        UniformBuffer::new(&mut *mapping).write(&uniforms).unwrap();
        drop(mapping);
        uniform_buffer.unmap();

        bgb.append_buffer(&uniform_buffer);

        ctx.data_core.directional_light_manager.add_to_bg(&mut bgb);
        ctx.data_core.point_light_manager.add_to_bg(&mut bgb);

        let shadow_uniform_bg = bgb.build(
            &ctx.renderer.device,
            Some("shadow uniform bg"),
            &binding_handles.interfaces.depth_uniform_bgl,
        );

        bgb.append_texture_view(shadow_target);

        let forward_uniform_bg = bgb.build(
            &ctx.renderer.device,
            Some("forward uniform bg"),
            &binding_handles.interfaces.forward_uniform_bgl,
        );

        ctx.graph_data.set_data(shadow_handle, Some(shadow_uniform_bg));
        ctx.graph_data.set_data(forward_handle, Some(forward_uniform_bg));
    })
}
