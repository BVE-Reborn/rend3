//! Starter RenderGraph that can be easily extended.
//!
//! This is a fully put together pipeline to render with rend3. If you don't
//! need any customization, this should be drop in without worrying about it.
//!
//! In order to start customizing it, copy the contents of
//! [`BaseRenderGraph::add_to_graph`] into your own code and start modifying it.
//! This will allow you to insert your own routines and customize the behavior
//! of the existing routines.
//!
//! [`BaseRenderGraphIntermediateState`] intentionally has all of its members
//! public. If you want to change what rendergraph image things are rendering
//! to, or muck with any of the data in there, you are free to, and the
//! following routines will behave as you configure.

use glam::{UVec2, Vec4};
use rend3::{
    format_sso,
    graph::{DataHandle, ReadyData, RenderGraph, RenderTargetDescriptor, RenderTargetHandle},
    types::{SampleCount, TextureFormat, TextureUsages},
    Renderer, ShaderPreProcessor,
};
use wgpu::{BindGroup, Buffer};

use crate::{
    common, culling,
    forward::RoutineAddToGraphArgs,
    pbr,
    skinning::{self, GpuSkinner, SkinningOutput},
    skybox, tonemapping,
};

/// Starter RenderGraph.
///
/// See module for documentation.
pub struct BaseRenderGraph {
    pub interfaces: common::WholeFrameInterfaces,
    pub samplers: common::Samplers,
    pub gpu_culler: culling::GpuCuller,
    pub gpu_skinner: GpuSkinner,
}

impl BaseRenderGraph {
    pub fn new(renderer: &Renderer, spp: &ShaderPreProcessor) -> Self {
        profiling::scope!("DefaultRenderGraphData::new");

        let interfaces = common::WholeFrameInterfaces::new(&renderer.device);

        let samplers = common::Samplers::new(&renderer.device);

        // TODO: Support more materials
        let gpu_culler = culling::GpuCuller::new::<pbr::PbrMaterial>(&renderer, spp);

        let gpu_skinner = GpuSkinner::new(&renderer.device, spp);

        Self {
            interfaces,
            samplers,
            gpu_culler,
            gpu_skinner,
        }
    }

    /// Add this to the rendergraph. This is the function you should start
    /// customizing.
    #[allow(clippy::too_many_arguments)]
    pub fn add_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        ready: &ReadyData,
        pbr: &'node crate::pbr::PbrRoutine,
        skybox: Option<&'node crate::skybox::SkyboxRoutine>,
        tonemapping: &'node crate::tonemapping::TonemappingRoutine,
        resolution: UVec2,
        samples: SampleCount,
        ambient: Vec4,
        clear_color: Vec4,
    ) {
        // Create intermediate storage
        let state = BaseRenderGraphIntermediateState::new(graph, ready, resolution, samples);

        // Preparing and uploading data
        state.pre_skinning(graph);
        state.create_frame_uniforms(graph, self, ambient, resolution);

        // Skinning
        state.skinning(graph, self);

        // Culling
        state.pbr_shadow_culling(graph, self);
        state.pbr_culling(graph, self);

        // Clear targets
        state.clear(graph, clear_color);

        // Depth-only rendering
        state.pbr_shadow_rendering(graph, pbr);

        // Skybox
        state.skybox(graph, skybox, samples);

        // Forward rendering
        state.pbr_forward_rendering(graph, pbr, samples);

        // Make the reference to the surface
        let surface = graph.add_surface_texture();
        state.tonemapping(graph, tonemapping, surface);
    }
}

/// Struct that globs all the information the [`BaseRenderGraph`] needs.
///
/// This is intentionally public so all this can be changed by the user if they
/// so desire.
pub struct BaseRenderGraphIntermediateState {
    pub pre_cull: DataHandle<Buffer>,
    pub shadow_cull: Vec<DataHandle<culling::DrawCallSet>>,
    pub cull: DataHandle<culling::DrawCallSet>,

    pub shadow_uniform_bg: DataHandle<BindGroup>,
    pub forward_uniform_bg: DataHandle<BindGroup>,
    pub color: RenderTargetHandle,
    pub resolve: Option<RenderTargetHandle>,
    pub depth: RenderTargetHandle,
    pub pre_skinning_buffers: DataHandle<skinning::PreSkinningBuffers>,
    pub skinned_data: DataHandle<skinning::SkinningOutput>,
}
impl BaseRenderGraphIntermediateState {
    /// Create the default setting for all state.
    pub fn new(graph: &mut RenderGraph<'_>, ready: &ReadyData, resolution: UVec2, samples: SampleCount) -> Self {
        // We need to know how many shadows we need to render
        // TODO
        let shadow_count = 1;

        // Create global bind group information
        let shadow_uniform_bg = graph.add_data::<BindGroup>();
        let forward_uniform_bg = graph.add_data::<BindGroup>();

        // Make the actual render targets we want to render to.
        let color = graph.add_render_target(RenderTargetDescriptor {
            label: Some("hdr color".into()),
            resolution,
            samples,
            format: TextureFormat::Rgba16Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        });
        let resolve = samples.needs_resolve().then(|| {
            graph.add_render_target(RenderTargetDescriptor {
                label: Some("hdr resolve".into()),
                resolution,
                samples: SampleCount::One,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            })
        });
        let depth = graph.add_render_target(RenderTargetDescriptor {
            label: Some("hdr depth".into()),
            resolution,
            samples,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT,
        });

        let pre_skinning_buffers = graph.add_data::<skinning::PreSkinningBuffers>();
        let skinned_data = graph.add_data::<SkinningOutput>();

        Self {
            pre_cull: graph.add_data(),
            shadow_cull: {
                let mut shadows = Vec::with_capacity(shadow_count);
                shadows.resize_with(shadow_count, || graph.add_data());
                shadows
            },
            cull: graph.add_data(),

            shadow_uniform_bg,
            forward_uniform_bg,
            color,
            resolve,
            depth,
            pre_skinning_buffers,
            skinned_data,
        }
    }

    /// Upload skinning input data to the GPU.
    pub fn pre_skinning(&self, graph: &mut RenderGraph<'_>) {
        crate::skinning::add_pre_skin_to_graph(graph, self.pre_skinning_buffers);
    }

    /// Create all the uniforms all the shaders in this graph need.
    pub fn create_frame_uniforms<'node>(
        &self,
        graph: &mut RenderGraph<'node>,
        base: &'node BaseRenderGraph,
        ambient: Vec4,
        resolution: UVec2,
    ) {
        crate::uniforms::add_to_graph(
            graph,
            self.shadow_uniform_bg,
            self.forward_uniform_bg,
            &base.interfaces,
            &base.samplers,
            ambient,
            resolution,
        );
    }

    /// Does all shadow culling for the PBR materials.
    pub fn pbr_shadow_culling<'node>(&self, graph: &mut RenderGraph<'node>, base: &'node BaseRenderGraph) {
        for (shadow_index, &shadow_culled) in self.shadow_cull.iter().enumerate() {
            crate::culling::add_culling_to_graph::<pbr::PbrMaterial>(
                graph,
                shadow_culled,
                &base.gpu_culler,
                &format_sso!("Shadow Culling S{}", shadow_index),
            );
        }
    }

    pub fn skinning<'node>(&self, graph: &mut RenderGraph<'node>, base: &'node BaseRenderGraph) {
        crate::skinning::add_skinning_to_graph(graph, &base.gpu_skinner, self.pre_skinning_buffers, self.skinned_data);
    }

    /// Does all culling for the forward PBR materials.
    pub fn pbr_culling<'node>(&self, graph: &mut RenderGraph<'node>, base: &'node BaseRenderGraph) {
        crate::culling::add_culling_to_graph::<pbr::PbrMaterial>(graph, self.cull, &base.gpu_culler, "Primary Culling");
    }

    /// Clear all the targets to their needed values
    pub fn clear<'node>(&self, graph: &mut RenderGraph<'node>, clear_color: Vec4) {
        crate::clear::add_clear_to_graph(graph, self.color, self.resolve, self.depth, clear_color, 0.0);
    }

    /// Render all shadows for the PBR materials.
    pub fn pbr_shadow_rendering<'node>(&self, graph: &mut RenderGraph<'node>, pbr: &'node pbr::PbrRoutine) {
        for (shadow_index, &shadow_culled) in self.shadow_cull.iter().enumerate() {
            pbr.opaque_depth.add_forward_to_graph(RoutineAddToGraphArgs {
                graph,
                whole_frame_uniform_bg: self.shadow_uniform_bg,
                culled: shadow_culled,
                per_material: &pbr.per_material,
                extra_bgs: None,
                label: &format!("pbr shadow renderering S{shadow_index}"),
                samples: SampleCount::One,
                color: self.color,
                resolve: self.resolve,
                depth: self.depth,
            });
        }
    }

    /// Render the skybox.
    pub fn skybox<'node>(
        &self,
        graph: &mut RenderGraph<'node>,
        skybox: Option<&'node skybox::SkyboxRoutine>,
        samples: SampleCount,
    ) {
        if let Some(skybox) = skybox {
            skybox.add_to_graph(
                graph,
                self.color,
                self.resolve,
                self.depth,
                self.forward_uniform_bg,
                samples,
            );
        }
    }

    /// Render the PBR materials.
    pub fn pbr_forward_rendering<'node>(
        &self,
        graph: &mut RenderGraph<'node>,
        pbr: &'node pbr::PbrRoutine,
        samples: SampleCount,
    ) {
        pbr.opaque_routine.add_forward_to_graph(RoutineAddToGraphArgs {
            graph,
            whole_frame_uniform_bg: self.forward_uniform_bg,
            culled: self.cull,
            per_material: &pbr.per_material,
            extra_bgs: None,
            label: "PBR Forward",
            samples,
            color: self.color,
            resolve: self.resolve,
            depth: self.depth,
        });
    }

    /// Tonemap onto the given render target.
    pub fn tonemapping<'node>(
        &self,
        graph: &mut RenderGraph<'node>,
        tonemapping: &'node tonemapping::TonemappingRoutine,
        target: RenderTargetHandle,
    ) {
        tonemapping.add_to_graph(
            graph,
            self.resolve.unwrap_or(self.color),
            target,
            self.forward_uniform_bg,
        );
    }
}
