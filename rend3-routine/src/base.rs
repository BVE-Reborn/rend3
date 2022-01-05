use glam::{UVec2, Vec4};
use rend3::{
    format_sso,
    types::{SampleCount, TextureFormat, TextureUsages},
    DataHandle, ModeData, ReadyData, RenderGraph, RenderTargetDescriptor, Renderer,
};
use wgpu::{BindGroup, Buffer};

use crate::{common, culling, pbr};

struct PerTransparencyInfo {
    ty: pbr::TransparencyType,
    pre_cull: DataHandle<Buffer>,
    shadow_cull: Vec<DataHandle<culling::PerMaterialData>>,
    cull: DataHandle<culling::PerMaterialData>,
}

pub struct BaseRenderGraph {
    pub interfaces: common::GenericShaderInterfaces,
    pub samplers: common::Samplers,
    pub gpu_culler: ModeData<(), culling::GpuCuller>,
}

impl BaseRenderGraph {
    pub fn new(renderer: &Renderer) -> Self {
        profiling::scope!("DefaultRenderGraphData::new");

        let interfaces = common::GenericShaderInterfaces::new(&renderer.device);

        let samplers = common::Samplers::new(&renderer.device);

        let gpu_culler = renderer
            .mode
            .into_data(|| (), || culling::GpuCuller::new(&renderer.device));

        Self {
            interfaces,
            samplers,
            gpu_culler,
        }
    }

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
        // We need to know how many shadows we need to render
        let shadow_count = ready.directional_light_cameras.len();

        // Setup all of our per-transparency data
        let mut per_transparency = Vec::with_capacity(3);
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

        // A lot of things don't deal with blending, so lets make a subslice for that situation.
        let per_transparency_no_blend = &per_transparency[..2];

        // Add pre-culling
        for trans in &per_transparency {
            crate::pre_cull::add_to_graph::<pbr::PbrMaterial>(
                graph,
                trans.ty as u64,
                trans.ty.to_sorting(),
                &format_sso!("{:?}", trans.ty),
                trans.pre_cull,
            );
        }

        // Create global bind group information
        let shadow_uniform_bg = graph.add_data::<BindGroup>();
        let forward_uniform_bg = graph.add_data::<BindGroup>();
        crate::uniforms::add_to_graph(
            graph,
            shadow_uniform_bg,
            forward_uniform_bg,
            &self.interfaces,
            &self.samplers,
            ambient,
        );

        // Add shadow culling
        for trans in per_transparency_no_blend {
            for (shadow_index, &shadow_culled) in trans.shadow_cull.iter().enumerate() {
                crate::culling::add_culling_to_graph::<pbr::PbrMaterial>(
                    graph,
                    trans.pre_cull,
                    shadow_culled,
                    &pbr.per_material,
                    &self.gpu_culler,
                    Some(shadow_index),
                    trans.ty as u64,
                    trans.ty.to_sorting(),
                    &format_sso!("Shadow Culling S{} {:?}", shadow_index, trans.ty),
                );
            }
        }

        // Add primary culling
        for trans in &per_transparency {
            crate::culling::add_culling_to_graph::<pbr::PbrMaterial>(
                graph,
                trans.pre_cull,
                trans.cull,
                &pbr.per_material,
                &self.gpu_culler,
                None,
                trans.ty as u64,
                trans.ty.to_sorting(),
                &format_sso!("Primary Culling {:?}", trans.ty),
            );
        }

        // Add shadow rendering
        for trans in per_transparency_no_blend {
            for (shadow_index, &shadow_culled) in trans.shadow_cull.iter().enumerate() {
                pbr.depth_pipelines.add_shadow_rendering_to_graph(
                    graph,
                    matches!(trans.ty, pbr::TransparencyType::Cutout),
                    shadow_index,
                    shadow_uniform_bg,
                    shadow_culled,
                );
            }
        }

        // Make the actual render targets we want to render to.
        let color = graph.add_render_target(RenderTargetDescriptor {
            label: Some("hdr color".into()),
            dim: resolution,
            samples,
            format: TextureFormat::Rgba16Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        });
        let resolve = samples.needs_resolve().then(|| {
            graph.add_render_target(RenderTargetDescriptor {
                label: Some("hdr resolve".into()),
                dim: resolution,
                samples: SampleCount::One,
                format: TextureFormat::Rgba16Float,
                usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            })
        });
        let depth = graph.add_render_target(RenderTargetDescriptor {
            label: Some("hdr depth".into()),
            dim: resolution,
            samples,
            format: TextureFormat::Depth32Float,
            usage: TextureUsages::RENDER_ATTACHMENT,
        });

        // Add depth prepass
        for trans in per_transparency_no_blend {
            pbr.depth_pipelines.add_prepass_to_graph(
                graph,
                forward_uniform_bg,
                trans.cull,
                samples,
                matches!(trans.ty, pbr::TransparencyType::Cutout),
                color,
                resolve,
                depth,
            );
        }

        // Add skybox
        if let Some(skybox) = skybox {
            skybox.add_to_graph(graph, color, resolve, depth, forward_uniform_bg, samples);
        }

        // Add primary rendering
        for trans in &per_transparency {
            pbr.add_forward_to_graph(
                graph,
                forward_uniform_bg,
                trans.cull,
                samples,
                trans.ty,
                color,
                resolve,
                depth,
            );
        }

        // Make the reference to the surface
        let surface = graph.add_surface_texture();

        tonemapping.add_to_graph(graph, resolve.unwrap_or(color), surface, forward_uniform_bg);
    }
}
