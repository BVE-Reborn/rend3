/// PBR Render Routine for rend3.
/// Contains [`PbrMaterial`] and the [`PbrRenderRoutine`] which serve as the default render routines.
///
/// Tries to strike a balance between photorealism and performance.
use glam::Vec4;
use rend3::{
    format_sso,
    types::{Handedness, SampleCount, TextureFormat, TextureUsages},
    DataHandle, DepthHandle, ModeData, ReadyData, RenderGraph, RenderPassDepthTarget, RenderPassTarget,
    RenderPassTargets, RenderTargetDescriptor, RenderTargetHandle, Renderer, RendererDataCore,
};
use wgpu::{BindGroup, Buffer, Color, Features, RenderPipeline};

pub use utils::*;

use crate::{
    common::{interfaces::ShaderInterfaces, samplers::Samplers},
    culling::{gpu::GpuCuller, CulledObjectSet},
    depth::DepthPipelines,
    material::{PbrMaterial, TransparencyType},
};

pub mod common;
pub mod culling;
pub mod depth;
pub mod material;
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

/// Render routine that renders the using PBR materials and gpu based culling.
pub struct PbrRenderRoutine {
    pub primary_passes: PrimaryPasses,
    pub depth_pipelines: DepthPipelines,

    pub render_texture_options: RenderTextureOptions,
}

impl PbrRenderRoutine {
    pub fn new(
        renderer: &Renderer,
        data_core: &mut RendererDataCore,
        interfaces: &ShaderInterfaces,
        render_texture_options: RenderTextureOptions,
    ) -> Self {
        profiling::scope!("PbrRenderRoutine::new");

        data_core
            .material_manager
            .ensure_archetype::<PbrMaterial>(&renderer.device, renderer.mode);

        let unclipped_depth_supported = renderer.features.contains(Features::DEPTH_CLIP_CONTROL);

        let depth_pipelines =
            DepthPipelines::new::<PbrMaterial>(renderer, data_core, interfaces, unclipped_depth_supported);

        let primary_passes = {
            PrimaryPasses::new(PrimaryPassesNewArgs {
                renderer,
                data_core,
                interfaces,
                handedness: renderer.handedness,
                samples: render_texture_options.samples,
                unclipped_depth_supported,
            })
        };

        Self {
            depth_pipelines,
            primary_passes,

            render_texture_options,
        }
    }

    pub fn resize(&mut self, renderer: &Renderer, interfaces: &ShaderInterfaces, options: RenderTextureOptions) {
        profiling::scope!("PbrRenderRoutine::resize");
        let different_sample_count = self.render_texture_options.samples != options.samples;
        if different_sample_count {
            let mut data_core = renderer.data_core.lock();
            let data_core = &mut *data_core;
            // TODO(material): figure out a better way for zero materials to work
            data_core
                .material_manager
                .ensure_archetype::<PbrMaterial>(&renderer.device, renderer.mode);
            PrimaryPasses::new(PrimaryPassesNewArgs {
                renderer,
                data_core,
                interfaces,
                handedness: renderer.handedness,
                samples: options.samples,
                unclipped_depth_supported: renderer.features.contains(Features::DEPTH_CLIP_CONTROL),
            });
        }
        self.render_texture_options = options;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_forward_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        transparency: TransparencyType,
        color: RenderTargetHandle,
        resolve: Option<RenderTargetHandle>,
        depth: RenderTargetHandle,
        forward_uniform_bg: DataHandle<BindGroup>,
        culled: DataHandle<CulledPerMaterial>,
    ) {
        let mut builder = graph.add_node(format_sso!("Primary Forward {:?}", transparency));

        let hdr_color_handle = builder.add_render_target_output(color);
        let hdr_resolve = builder.add_optional_render_target_output(resolve);
        let hdr_depth_handle = builder.add_render_target_output(depth);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: hdr_color_handle,
                clear: Color::BLACK,
                resolve: hdr_resolve,
            }],
            depth_stencil: Some(RenderPassDepthTarget {
                target: DepthHandle::RenderTarget(hdr_depth_handle),
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        let _ = builder.add_shadow_array_input();

        let forward_uniform_handle = builder.add_data_input(forward_uniform_bg);
        let cull_handle = builder.add_data_input(culled);

        let pt_handle = builder.passthrough_ref(self);

        builder.build(move |pt, _renderer, encoder_or_pass, temps, ready, graph_data| {
            let this = pt.get(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_handle).unwrap();
            let culled = graph_data.get_data(temps, cull_handle).unwrap();

            let pipeline = match transparency {
                TransparencyType::Opaque => &this.primary_passes.forward_opaque,
                TransparencyType::Cutout => &this.primary_passes.forward_cutout,
                TransparencyType::Blend => &this.primary_passes.forward_blend,
            };

            graph_data.mesh_manager.buffers().bind(rpass);

            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, forward_uniform_bg, &[]);
            rpass.set_bind_group(1, &culled.per_material, &[]);

            match culled.inner.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(rpass, draws, graph_data.material_manager, 2),
                ModeData::GPU(ref data) => {
                    rpass.set_bind_group(2, ready.d2_texture.bg.as_gpu(), &[]);
                    culling::gpu::run(rpass, data);
                }
            }
        });
    }
}

pub struct PrimaryPassesNewArgs<'a> {
    pub renderer: &'a Renderer,
    pub data_core: &'a mut RendererDataCore,

    pub interfaces: &'a common::interfaces::ShaderInterfaces,

    pub handedness: Handedness,
    pub samples: SampleCount,
    pub unclipped_depth_supported: bool,
}

pub struct PrimaryPasses {
    forward_blend: RenderPipeline,
    forward_cutout: RenderPipeline,
    forward_opaque: RenderPipeline,
}
impl PrimaryPasses {
    pub fn new(args: PrimaryPassesNewArgs<'_>) -> Self {
        profiling::scope!("PrimaryPasses::new");

        args.data_core
            .material_manager
            .ensure_archetype::<PbrMaterial>(&args.renderer.device, args.renderer.mode);

        let forward_pass_args = common::forward_pass::BuildForwardPassShaderArgs {
            renderer: args.renderer,
            data_core: args.data_core,
            interfaces: args.interfaces,
            samples: args.samples,
            transparency: TransparencyType::Opaque,
        };
        let forward_opaque = common::forward_pass::build_forward_pass_pipeline(forward_pass_args.clone());
        let forward_cutout =
            common::forward_pass::build_forward_pass_pipeline(common::forward_pass::BuildForwardPassShaderArgs {
                transparency: TransparencyType::Cutout,
                ..forward_pass_args.clone()
            });
        let forward_blend =
            common::forward_pass::build_forward_pass_pipeline(common::forward_pass::BuildForwardPassShaderArgs {
                transparency: TransparencyType::Blend,
                ..forward_pass_args.clone()
            });
        Self {
            forward_blend,
            forward_cutout,
            forward_opaque,
        }
    }
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
    pbr: &'node PbrRenderRoutine,
    skybox: Option<&'node skybox::SkyboxRoutine>,
    tonemapping: &'node tonemapping::TonemappingRoutine,
    data: &'node DefaultRenderGraphData,
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
        dim: pbr.render_texture_options.resolution,
        samples,
        format: TextureFormat::Rgba16Float,
        usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
    });
    let resolve = samples.needs_resolve().then(|| {
        graph.add_render_target(RenderTargetDescriptor {
            label: Some("hdr resolve".into()),
            dim: pbr.render_texture_options.resolution,
            samples: SampleCount::One,
            format: TextureFormat::Rgba16Float,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
        })
    });
    let depth = graph.add_render_target(RenderTargetDescriptor {
        label: Some("hdr depth".into()),
        dim: pbr.render_texture_options.resolution,
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
        skybox.add_to_graph(graph, color, resolve, depth, forward_uniform_bg);
    }

    // Add primary rendering
    for trans in &per_transparency {
        pbr.add_forward_to_graph(graph, trans.ty, color, resolve, depth, forward_uniform_bg, trans.cull);
    }

    // Make the reference to the surface
    let surface = graph.add_surface_texture();

    tonemapping.add_to_graph(graph, resolve.unwrap_or(color), surface, forward_uniform_bg);
}
