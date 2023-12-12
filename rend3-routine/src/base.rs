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

use std::{iter::zip, sync::Arc};

use glam::{UVec2, Vec4};
use rend3::{
    format_sso,
    graph::{
        DataHandle, InstructionEvaluationOutput, RenderGraph, RenderTargetDescriptor, RenderTargetHandle, ViewportRect,
    },
    types::{SampleCount, TextureFormat, TextureUsages},
    Renderer, ShaderPreProcessor, INTERNAL_SHADOW_DEPTH_FORMAT,
};
use wgpu::{BindGroup, Buffer};

use crate::{
    common::{self, CameraIndex},
    culling,
    forward::RoutineAddToGraphArgs,
    pbr, skinning,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct DepthTargets {
    pub single_sample_mipped: RenderTargetHandle,
    pub multi_sample: Option<RenderTargetHandle>,
}

impl DepthTargets {
    pub fn new(graph: &mut RenderGraph<'_>, resolution: UVec2, samples: SampleCount) -> Self {
        let single_sample_mipped = graph.add_render_target(RenderTargetDescriptor {
            label: Some("hdr depth".into()),
            resolution,
            depth: 1,
            mip_levels: None,
            samples: SampleCount::One,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        });

        let multi_sample = samples.needs_resolve().then(|| {
            graph.add_render_target(RenderTargetDescriptor {
                label: Some("hdr depth multisampled".into()),
                resolution,
                depth: 1,
                mip_levels: Some(1),
                samples,
                format: TextureFormat::Depth32Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            })
        });

        Self {
            single_sample_mipped,
            multi_sample,
        }
    }

    pub fn rendering_target(&self) -> RenderTargetHandle {
        self.multi_sample.unwrap_or(self.single_sample_mipped.set_mips(0..1))
    }
}

pub struct BaseRenderGraphInputs<'a, 'node> {
    pub eval_output: &'a InstructionEvaluationOutput,
    pub pbr: &'node crate::pbr::PbrRoutine,
    pub skybox: Option<&'node crate::skybox::SkyboxRoutine>,
    pub tonemapping: &'node crate::tonemapping::TonemappingRoutine,
    pub target_texture: RenderTargetHandle,
    pub resolution: UVec2,
    pub samples: SampleCount,
}

#[derive(Debug, Default)]
pub struct BaseRenderGraphSettings {
    pub ambient: Vec4,
    pub clear_color: Vec4,
}

/// Starter RenderGraph.
///
/// See module for documentation.
pub struct BaseRenderGraph {
    pub interfaces: common::WholeFrameInterfaces,
    pub samplers: common::Samplers,
    pub gpu_culler: culling::GpuCuller,
    pub gpu_skinner: skinning::GpuSkinner,
}

impl BaseRenderGraph {
    pub fn new(renderer: &Arc<Renderer>, spp: &ShaderPreProcessor) -> Self {
        profiling::scope!("DefaultRenderGraphData::new");

        let interfaces = common::WholeFrameInterfaces::new(&renderer.device);

        let samplers = common::Samplers::new(&renderer.device);

        // TODO: Support more materials
        let gpu_culler = culling::GpuCuller::new::<pbr::PbrMaterial>(renderer, spp);

        let gpu_skinner = skinning::GpuSkinner::new(&renderer.device, spp);

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
        inputs: BaseRenderGraphInputs<'_, 'node>,
        settings: BaseRenderGraphSettings,
    ) {
        // Create the data and handles for the graph.
        let mut state = BaseRenderGraphIntermediateState::new(graph, inputs, settings);

        // Clear the shadow map.
        state.clear_shadow();

        // Prepare all the uniforms that all shaders need access to.
        state.create_frame_uniforms(self);

        // Perform compute based skinning.
        state.skinning(self);

        // Upload the uniforms for the objects in the shadow pass.
        state.shadow_object_uniform_upload(self);
        // Perform culling for the objects in the shadow pass.
        state.pbr_shadow_culling(self);

        // Render all the shadows to the shadow map.
        state.pbr_shadow_rendering();

        // Clear the primary render target and depth target.
        state.clear();

        // Upload the uniforms for the objects in the forward pass.
        state.object_uniform_upload(self);

        // Do the first pass, rendering the predicted triangles from last frame.
        state.pbr_render_opaque_predicted_triangles();

        // Create the hi-z buffer.
        state.hi_z();

        // Perform culling for the objects in the forward pass.
        //
        // The result of culling will be used to predict the visible triangles for
        // the next frame. It will also render all the triangles that were visible
        // but were not predicted last frame.
        state.pbr_culling(self);

        // Do the second pass, rendering the residual triangles.
        state.pbr_render_opaque_residual_triangles();

        // Render the skybox.
        state.skybox();

        // Render all transparent objects.
        //
        // This _must_ happen after culling, as all transparent objects are
        // considered "residual".
        state.pbr_forward_rendering_transparent();

        // Tonemap the HDR inner buffer to the output buffer.
        state.tonemapping();
    }
}

/// Struct that globs all the information the [`BaseRenderGraph`] needs.
///
/// This is intentionally public so all this can be changed by the user if they
/// so desire.
pub struct BaseRenderGraphIntermediateState<'a, 'node> {
    pub graph: &'a mut RenderGraph<'node>,
    pub inputs: BaseRenderGraphInputs<'a, 'node>,
    pub settings: BaseRenderGraphSettings,

    pub pre_cull: DataHandle<Buffer>,
    pub shadow_cull: Vec<DataHandle<Arc<culling::DrawCallSet>>>,
    pub cull: DataHandle<Arc<culling::DrawCallSet>>,

    pub shadow_uniform_bg: DataHandle<BindGroup>,
    pub forward_uniform_bg: DataHandle<BindGroup>,
    pub shadow: RenderTargetHandle,
    pub color: RenderTargetHandle,
    pub resolve: Option<RenderTargetHandle>,
    pub depth: DepthTargets,
    pub pre_skinning_buffers: DataHandle<skinning::PreSkinningBuffers>,
}
impl<'a, 'node> BaseRenderGraphIntermediateState<'a, 'node> {
    /// Create the default setting for all state.
    pub fn new(
        graph: &'a mut RenderGraph<'node>,
        inputs: BaseRenderGraphInputs<'a, 'node>,
        settings: BaseRenderGraphSettings,
    ) -> Self {
        // We need to know how many shadows we need to render
        let shadow_count = inputs.eval_output.shadows.len();

        // Create global bind group information
        let shadow_uniform_bg = graph.add_data::<BindGroup>();
        let forward_uniform_bg = graph.add_data::<BindGroup>();

        // Shadow render target
        let shadow = graph.add_render_target(RenderTargetDescriptor {
            label: Some("shadow target".into()),
            resolution: inputs.eval_output.shadow_target_size,
            depth: 1,
            mip_levels: Some(1),
            samples: SampleCount::One,
            format: INTERNAL_SHADOW_DEPTH_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        });

        // Make the actual render targets we want to render to.
        let color = graph.add_render_target(RenderTargetDescriptor {
            label: Some("hdr color".into()),
            resolution: inputs.resolution,
            depth: 1,
            samples: inputs.samples,
            mip_levels: Some(1),
            format: TextureFormat::Rgba16Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        });
        let resolve = inputs.samples.needs_resolve().then(|| {
            graph.add_render_target(RenderTargetDescriptor {
                label: Some("hdr resolve".into()),
                resolution: inputs.resolution,
                depth: 1,
                mip_levels: Some(1),
                samples: SampleCount::One,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            })
        });
        let depth = DepthTargets::new(graph, inputs.resolution, inputs.samples);

        let pre_skinning_buffers = graph.add_data::<skinning::PreSkinningBuffers>();

        let pre_cull = graph.add_data();
        let mut shadow_cull = Vec::with_capacity(shadow_count);
        shadow_cull.resize_with(shadow_count, || graph.add_data());
        let cull = graph.add_data();
        Self {
            graph,
            inputs,
            settings,

            pre_cull,
            shadow_cull,
            cull,

            shadow_uniform_bg,
            forward_uniform_bg,
            shadow,
            color,
            resolve,
            depth,
            pre_skinning_buffers,
        }
    }

    /// Create all the uniforms all the shaders in this graph need.
    pub fn create_frame_uniforms(&mut self, base: &'node BaseRenderGraph) {
        crate::uniforms::add_to_graph(
            self.graph,
            self.shadow_uniform_bg,
            self.forward_uniform_bg,
            self.shadow,
            &base.interfaces,
            &base.samplers,
            self.settings.ambient,
            self.inputs.resolution,
        );
    }

    pub fn shadow_object_uniform_upload(&mut self, base: &'node BaseRenderGraph) {
        for (shadow_index, shadow) in self.inputs.eval_output.shadows.iter().enumerate() {
            base.gpu_culler.add_object_uniform_upload_to_graph::<pbr::PbrMaterial>(
                self.graph,
                CameraIndex::Shadow(shadow_index as u32),
                UVec2::splat(shadow.map.size),
                SampleCount::One,
                &format_sso!("Shadow Culling S{}", shadow_index),
            );
        }
    }

    /// Does all shadow culling for the PBR materials.
    pub fn pbr_shadow_culling(&mut self, base: &'node BaseRenderGraph) {
        for (shadow_index, &shadow_culled) in self.shadow_cull.iter().enumerate() {
            base.gpu_culler.add_culling_to_graph::<pbr::PbrMaterial>(
                self.graph,
                shadow_culled,
                self.shadow,
                CameraIndex::Shadow(shadow_index as u32),
                &format_sso!("Shadow Culling S{}", shadow_index),
            );
        }
    }

    pub fn skinning(&mut self, base: &'node BaseRenderGraph) {
        skinning::add_skinning_to_graph(self.graph, &base.gpu_skinner);
    }

    pub fn object_uniform_upload(&mut self, base: &'node BaseRenderGraph) {
        base.gpu_culler.add_object_uniform_upload_to_graph::<pbr::PbrMaterial>(
            self.graph,
            CameraIndex::Viewport,
            self.inputs.resolution,
            self.inputs.samples,
            "Uniform Bake",
        );
    }

    /// Does all culling for the forward PBR materials.
    pub fn pbr_culling(&mut self, base: &'node BaseRenderGraph) {
        base.gpu_culler.add_culling_to_graph::<pbr::PbrMaterial>(
            self.graph,
            self.cull,
            self.depth.single_sample_mipped,
            CameraIndex::Viewport,
            "Primary Culling",
        );
    }

    /// Clear all the targets to their needed values
    pub fn clear_shadow(&mut self) {
        crate::clear::add_clear_to_graph(self.graph, None, None, self.shadow, Vec4::ZERO, 0.0);
    }

    /// Clear all the targets to their needed values
    pub fn clear(&mut self) {
        crate::clear::add_clear_to_graph(
            self.graph,
            Some(self.color),
            self.resolve,
            self.depth.rendering_target(),
            self.settings.clear_color,
            0.0,
        );
    }

    /// Render all shadows for the PBR materials.
    pub fn pbr_shadow_rendering(&mut self) {
        let iter = zip(&self.shadow_cull, &self.inputs.eval_output.shadows);
        for (shadow_index, (shadow_cull, desc)) in iter.enumerate() {
            let routines = [&self.inputs.pbr.opaque_depth, &self.inputs.pbr.cutout_depth];
            for routine in routines {
                routine.add_forward_to_graph(RoutineAddToGraphArgs {
                    graph: self.graph,
                    whole_frame_uniform_bg: self.shadow_uniform_bg,
                    culling_output_handle: Some(*shadow_cull),
                    per_material: &self.inputs.pbr.per_material,
                    extra_bgs: None,
                    label: &format!("pbr shadow renderering S{shadow_index}"),
                    samples: SampleCount::One,
                    camera: CameraIndex::Shadow(shadow_index as u32),
                    color: None,
                    resolve: None,
                    depth: self
                        .shadow
                        .set_viewport(ViewportRect::new(desc.map.offset, UVec2::splat(desc.map.size))),
                });
            }
        }
    }

    /// Render the skybox.
    pub fn skybox(&mut self) {
        if let Some(skybox) = self.inputs.skybox {
            skybox.add_to_graph(
                self.graph,
                self.color,
                self.resolve,
                self.depth.rendering_target(),
                self.forward_uniform_bg,
                self.inputs.samples,
            );
        }
    }

    /// Render the PBR materials.
    pub fn pbr_render_opaque_predicted_triangles(&mut self) {
        let routines = [&self.inputs.pbr.opaque_routine, &self.inputs.pbr.cutout_routine];
        for routine in routines {
            routine.add_forward_to_graph(RoutineAddToGraphArgs {
                graph: self.graph,
                whole_frame_uniform_bg: self.forward_uniform_bg,
                culling_output_handle: None,
                per_material: &self.inputs.pbr.per_material,
                extra_bgs: None,
                label: "PBR Forward Pass 1",
                samples: self.inputs.samples,
                camera: CameraIndex::Viewport,
                color: Some(self.color),
                resolve: self.resolve,
                depth: self.depth.rendering_target(),
            });
        }
    }

    /// Render the PBR materials.
    pub fn pbr_render_opaque_residual_triangles(&mut self) {
        let routines = [&self.inputs.pbr.opaque_routine, &self.inputs.pbr.cutout_routine];
        for routine in routines {
            routine.add_forward_to_graph(RoutineAddToGraphArgs {
                graph: self.graph,
                whole_frame_uniform_bg: self.forward_uniform_bg,
                culling_output_handle: Some(self.cull),
                per_material: &self.inputs.pbr.per_material,
                extra_bgs: None,
                label: "PBR Forward Pass 2",
                samples: self.inputs.samples,
                camera: CameraIndex::Viewport,
                color: Some(self.color),
                resolve: self.resolve,
                depth: self.depth.rendering_target(),
            });
        }
    }

    /// Render the PBR materials.
    pub fn pbr_forward_rendering_transparent(&mut self) {
        self.inputs
            .pbr
            .blend_routine
            .add_forward_to_graph(RoutineAddToGraphArgs {
                graph: self.graph,
                whole_frame_uniform_bg: self.forward_uniform_bg,
                culling_output_handle: Some(self.cull),
                per_material: &self.inputs.pbr.per_material,
                extra_bgs: None,
                label: "PBR Forward",
                camera: CameraIndex::Viewport,
                samples: self.inputs.samples,
                color: Some(self.color),
                resolve: self.resolve,
                depth: self.depth.rendering_target(),
            });
    }

    pub fn hi_z(&mut self) {
        self.inputs
            .pbr
            .hi_z
            .add_hi_z_to_graph(self.graph, self.depth, self.inputs.resolution);
    }

    /// Tonemap onto the given render target.
    pub fn tonemapping(&mut self) {
        self.inputs.tonemapping.add_to_graph(
            self.graph,
            self.resolve.unwrap_or(self.color),
            self.inputs.target_texture,
            self.forward_uniform_bg,
        );
    }
}
