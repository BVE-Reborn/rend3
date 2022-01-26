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

use arrayvec::ArrayVec;
use glam::{UVec2, Vec4};
use rend3::{
    format_sso,
    graph::{DataHandle, ReadyData, RenderGraph, RenderTargetDescriptor, RenderTargetHandle},
    types::{SampleCount, TextureFormat, TextureUsages},
    ProfileData, Renderer,
};
use wgpu::{BindGroup, Buffer};

use crate::{
    common, culling, pbr,
    skinning::{self, GpuSkinner, SkinningOutput},
    skybox, tonemapping,
};

/// Handles and information for a single type of transparency in the PBR
/// pipeline.
pub struct PerTransparencyInfo {
    ty: pbr::TransparencyType,
    pre_cull: DataHandle<Buffer>,
    shadow_cull: Vec<DataHandle<culling::PerMaterialArchetypeData>>,
    cull: DataHandle<culling::PerMaterialArchetypeData>,
}

/// Starter RenderGraph.
///
/// See module for documentation.
pub struct BaseRenderGraph {
    pub interfaces: common::WholeFrameInterfaces,
    pub samplers: common::Samplers,
    pub gpu_culler: ProfileData<(), culling::GpuCuller>,
    pub gpu_skinner: GpuSkinner,
}

impl BaseRenderGraph {
    pub fn new(renderer: &Renderer) -> Self {
        profiling::scope!("DefaultRenderGraphData::new");

        let interfaces = common::WholeFrameInterfaces::new(&renderer.device);

        let samplers = common::Samplers::new(&renderer.device);

        let gpu_culler = renderer
            .profile
            .into_data(|| (), || culling::GpuCuller::new(&renderer.device));

        let gpu_skinner = GpuSkinner::new(&renderer.device);

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
    ) {
        // Create intermediate storage
        let state = BaseRenderGraphIntermediateState::new(graph, ready, resolution, samples);

        // Preparing and uploading data
        state.pre_skinning(graph);
        state.pbr_pre_culling(graph);
        state.create_frame_uniforms(graph, self, ambient);

        // Skinning
        state.skinning(graph, self);

        // Culling
        state.pbr_shadow_culling(graph, self, pbr);
        state.pbr_culling(graph, self, pbr);

        // Depth-only rendering
        state.pbr_shadow_rendering(graph, pbr);
        state.pbr_prepass_rendering(graph, pbr, samples);

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
    pub per_transparency: ArrayVec<PerTransparencyInfo, 3>,
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
        let shadow_count = ready.directional_light_cameras.len();

        // Setup all of our per-transparency data
        let mut per_transparency = ArrayVec::new();
        for ty in [
            pbr::TransparencyType::Opaque,
            pbr::TransparencyType::Cutout,
            pbr::TransparencyType::Blend,
        ] {
            per_transparency.push(PerTransparencyInfo {
                ty,
                pre_cull: graph.add_data(),
                shadow_cull: {
                    let mut shadows = Vec::with_capacity(shadow_count);
                    shadows.resize_with(shadow_count, || graph.add_data());
                    shadows
                },
                cull: graph.add_data(),
            })
        }

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
            per_transparency,
            shadow_uniform_bg,
            forward_uniform_bg,
            color,
            resolve,
            depth,
            pre_skinning_buffers,
            skinned_data,
        }
    }

    /// Upload culling input data to the GPU.
    pub fn pbr_pre_culling(&self, graph: &mut RenderGraph<'_>) {
        for trans in &self.per_transparency {
            crate::pre_cull::add_to_graph::<pbr::PbrMaterial>(
                graph,
                trans.ty as u64,
                trans.ty.to_sorting(),
                &format_sso!("{:?}", trans.ty),
                trans.pre_cull,
            );
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
    ) {
        crate::uniforms::add_to_graph(
            graph,
            self.shadow_uniform_bg,
            self.forward_uniform_bg,
            &base.interfaces,
            &base.samplers,
            ambient,
        );
    }

    /// Does all shadow culling for the PBR materials.
    pub fn pbr_shadow_culling<'node>(
        &self,
        graph: &mut RenderGraph<'node>,
        base: &'node BaseRenderGraph,
        pbr: &'node pbr::PbrRoutine,
    ) {
        for trans in &self.per_transparency[0..2] {
            for (shadow_index, &shadow_culled) in trans.shadow_cull.iter().enumerate() {
                crate::culling::add_culling_to_graph::<pbr::PbrMaterial>(
                    graph,
                    trans.pre_cull,
                    shadow_culled,
                    self.skinned_data,
                    &pbr.per_material,
                    &base.gpu_culler,
                    Some(shadow_index),
                    trans.ty as u64,
                    trans.ty.to_sorting(),
                    &format_sso!("Shadow Culling S{} {:?}", shadow_index, trans.ty),
                );
            }
        }
    }

    pub fn skinning<'node>(&self, graph: &mut RenderGraph<'node>, base: &'node BaseRenderGraph) {
        crate::skinning::add_skinning_to_graph(graph, &base.gpu_skinner, self.pre_skinning_buffers, self.skinned_data);
    }

    /// Does all culling for the forward PBR materials.
    pub fn pbr_culling<'node>(
        &self,
        graph: &mut RenderGraph<'node>,
        base: &'node BaseRenderGraph,
        pbr: &'node pbr::PbrRoutine,
    ) {
        for trans in &self.per_transparency {
            crate::culling::add_culling_to_graph::<pbr::PbrMaterial>(
                graph,
                trans.pre_cull,
                trans.cull,
                self.skinned_data,
                &pbr.per_material,
                &base.gpu_culler,
                None,
                trans.ty as u64,
                trans.ty.to_sorting(),
                &format_sso!("Primary Culling {:?}", trans.ty),
            );
        }
    }

    /// Render all shadows for the PBR materials.
    pub fn pbr_shadow_rendering<'node>(&self, graph: &mut RenderGraph<'node>, pbr: &'node pbr::PbrRoutine) {
        for trans in &self.per_transparency[0..2] {
            for (shadow_index, &shadow_culled) in trans.shadow_cull.iter().enumerate() {
                pbr.depth_pipelines.add_shadow_rendering_to_graph(
                    graph,
                    matches!(trans.ty, pbr::TransparencyType::Cutout),
                    shadow_index,
                    self.shadow_uniform_bg,
                    shadow_culled,
                );
            }
        }
    }

    /// Render the depth prepass for all PBR materials,
    pub fn pbr_prepass_rendering<'node>(
        &self,
        graph: &mut RenderGraph<'node>,
        pbr: &'node pbr::PbrRoutine,
        samples: SampleCount,
    ) {
        for trans in &self.per_transparency[0..2] {
            pbr.depth_pipelines.add_prepass_to_graph(
                graph,
                self.forward_uniform_bg,
                trans.cull,
                samples,
                matches!(trans.ty, pbr::TransparencyType::Cutout),
                self.color,
                self.resolve,
                self.depth,
            );
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
        for trans in &self.per_transparency {
            let inner = match trans.ty {
                pbr::TransparencyType::Opaque => &pbr.opaque_routine,
                pbr::TransparencyType::Cutout => &pbr.cutout_routine,
                pbr::TransparencyType::Blend => &pbr.blend_routine,
            };

            inner.add_forward_to_graph(
                graph,
                self.forward_uniform_bg,
                trans.cull,
                None,
                &format_sso!("PBR Forward {:?}", trans.ty),
                samples,
                self.color,
                self.resolve,
                self.depth,
            );
        }
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
