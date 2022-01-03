/// PBR Render Routine for rend3.
/// Contains [`PbrMaterial`] and the [`PbrRenderRoutine`] which serve as the default render routines.
///
/// Tries to strike a balance between photorealism and performance.
use glam::{UVec2, Vec4};
use rend3::{
    format_sso,
    types::{SampleCount, TextureFormat, TextureUsages},
    DataHandle, ModeData, ReadyData, RenderGraph, RenderTargetDescriptor, Renderer,
};
use wgpu::{BindGroup, Buffer};

pub use utils::*;

use crate::{
    common::{interfaces::ShaderInterfaces, samplers::Samplers},
    culling::{gpu::GpuCuller, CulledObjectSet},
    material::{PbrMaterial, TransparencyType},
};

pub mod common;
pub mod culling;
pub mod depth;
pub mod material;
pub mod pbr;
pub mod pre_cull;
pub mod shaders;
pub mod skybox;
pub mod tonemapping;
pub mod uniforms;
mod utils;
pub mod vertex;

pub struct CulledPerMaterial {
    inner: CulledObjectSet,
    per_material: BindGroup,
}

struct PerTransparencyInfo {
    ty: TransparencyType,
    pre_cull: DataHandle<Buffer>,
    shadow_cull: Vec<DataHandle<CulledPerMaterial>>,
    cull: DataHandle<CulledPerMaterial>,
}

pub struct DefaultRenderGraphData {
    pub interfaces: ShaderInterfaces,
    pub samplers: Samplers,
    pub gpu_culler: ModeData<(), GpuCuller>,
}

impl DefaultRenderGraphData {
    pub fn new(renderer: &Renderer) -> Self {
        profiling::scope!("DefaultRenderGraphData::new");

        let interfaces = common::interfaces::ShaderInterfaces::new(&renderer.device, renderer.mode);

        let samplers = common::samplers::Samplers::new(&renderer.device);

        let gpu_culler = renderer
            .mode
            .into_data(|| (), || culling::gpu::GpuCuller::new(&renderer.device));

        Self {
            interfaces,
            samplers,
            gpu_culler,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn add_default_rendergraph<'node>(
    graph: &mut RenderGraph<'node>,
    ready: &ReadyData,
    pbr: &'node pbr::PbrRenderRoutine,
    skybox: Option<&'node skybox::SkyboxRoutine>,
    tonemapping: &'node tonemapping::TonemappingRoutine,
    data: &'node DefaultRenderGraphData,
    resolution: UVec2,
    samples: SampleCount,
    ambient: Vec4,
) {
    // We need to know how many shadows we need to render
    let shadow_count = ready.directional_light_cameras.len();

    // Setup all of our per-transparency data
    let mut per_transparency = Vec::with_capacity(3);
    for ty in [
        TransparencyType::Opaque,
        TransparencyType::Cutout,
        TransparencyType::Blend,
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
        pre_cull::add_to_graph::<PbrMaterial>(
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
    uniforms::add_to_graph(
        graph,
        shadow_uniform_bg,
        forward_uniform_bg,
        &data.interfaces,
        &data.samplers,
        ambient,
    );

    // Add shadow culling
    for trans in per_transparency_no_blend {
        for (shadow_index, &shadow_culled) in trans.shadow_cull.iter().enumerate() {
            culling::add_culling_to_graph::<PbrMaterial>(
                graph,
                trans.pre_cull,
                shadow_culled,
                &data.interfaces,
                &data.gpu_culler,
                Some(shadow_index),
                trans.ty as u64,
                trans.ty.to_sorting(),
                &format_sso!("Shadow Culling S{} {:?}", shadow_index, trans.ty),
            );
        }
    }

    // Add primary culling
    for trans in &per_transparency {
        culling::add_culling_to_graph::<PbrMaterial>(
            graph,
            trans.pre_cull,
            trans.cull,
            &data.interfaces,
            &data.gpu_culler,
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
                matches!(trans.ty, TransparencyType::Cutout),
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
            matches!(trans.ty, TransparencyType::Cutout),
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
