/// PBR Render Routine for rend3.
/// target: todo!(), clear: todo!(), resolve: todo!()  target: todo!(), clear: todo!(), resolve: todo!()  target: todo!(), clear: todo!(), resolve: todo!()  target: todo!(), clear: todo!(), resolve: todo!()
/// Contains [`PbrMaterial`] and the [`PbrRenderRoutine`] which serve as the default render routines.
///
/// Tries to strike a balance between photorealism and performance.
use glam::Vec4;
use rend3::{
    format_sso,
    types::{Handedness, SampleCount, TextureFormat, TextureUsages},
    util::bind_merge::BindGroupBuilder,
    DataHandle, DepthHandle, ModeData, ReadyData, RenderGraph, RenderPassDepthTarget, RenderPassTarget,
    RenderPassTargets, RenderTargetDescriptor, RenderTargetHandle, Renderer, RendererDataCore, RendererMode,
};
use wgpu::{BindGroup, Buffer, Color, Features, RenderPipeline};

pub use utils::*;

use crate::{
    culling::{cpu::CpuCullerCullArgs, gpu::GpuCullerCullArgs, CulledObjectSet},
    material::{PbrMaterial, TransparencyType},
};

pub mod common;
pub mod culling;
pub mod material;
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
    pub interfaces: common::interfaces::ShaderInterfaces,
    pub samplers: common::samplers::Samplers,
    pub cpu_culler: culling::cpu::CpuCuller,
    pub gpu_culler: ModeData<(), culling::gpu::GpuCuller>,
    pub primary_passes: PrimaryPasses,

    pub ambient: Vec4,

    pub render_texture_options: RenderTextureOptions,
}

impl PbrRenderRoutine {
    pub fn new(renderer: &Renderer, render_texture_options: RenderTextureOptions) -> Self {
        profiling::scope!("PbrRenderRoutine::new");

        let interfaces = common::interfaces::ShaderInterfaces::new(&renderer.device, renderer.mode);

        let samplers = common::samplers::Samplers::new(&renderer.device);

        let cpu_culler = culling::cpu::CpuCuller::new();
        let gpu_culler = renderer
            .mode
            .into_data(|| (), || culling::gpu::GpuCuller::new(&renderer.device));

        let primary_passes = {
            PrimaryPasses::new(PrimaryPassesNewArgs {
                renderer,
                data_core: &mut *renderer.data_core.lock(),
                interfaces: &interfaces,
                handedness: renderer.handedness,
                samples: render_texture_options.samples,
                unclipped_depth_supported: renderer.features.contains(Features::DEPTH_CLIP_CONTROL),
            })
        };

        Self {
            interfaces,
            samplers,
            cpu_culler,
            gpu_culler,
            primary_passes,

            ambient: Vec4::new(0.0, 0.0, 0.0, 1.0),

            render_texture_options,
        }
    }

    pub fn set_ambient_color(&mut self, color: Vec4) {
        self.ambient = color;
    }

    pub fn resize(&mut self, renderer: &Renderer, options: RenderTextureOptions) {
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
                interfaces: &self.interfaces,
                handedness: renderer.handedness,
                samples: options.samples,
                unclipped_depth_supported: renderer.features.contains(Features::DEPTH_CLIP_CONTROL),
            });
        }
        self.render_texture_options = options;
    }

    pub fn add_pre_cull_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        transparency: TransparencyType,
        pre_cull_data: DataHandle<Buffer>,
    ) {
        let mut builder = graph.add_node(format_sso!("pre-cull {:?}", transparency));
        let data_handle = builder.add_data_output(pre_cull_data);

        builder.build(move |_pt, renderer, _encoder_or_pass, _temps, _ready, graph_data| {
            let objects = graph_data
                .object_manager
                .get_objects::<PbrMaterial>(transparency as u64);
            let objects = common::sorting::sort_objects(objects, graph_data.camera_manager, transparency.to_sorting());
            let buffer = culling::gpu::build_cull_data(&renderer.device, &objects);
            graph_data.set_data::<Buffer>(data_handle, Some(buffer));
        });
    }

    pub fn add_uniform_bg_creation_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        shadow_uniform_bg: DataHandle<BindGroup>,
        forward_uniform_bg: DataHandle<BindGroup>,
    ) {
        let mut builder = graph.add_node("build uniform data");
        let shadow_handle = builder.add_data_output(shadow_uniform_bg);
        let forward_handle = builder.add_data_output(forward_uniform_bg);
        builder.build(move |_pt, renderer, _encoder_or_pass, _temps, _ready, graph_data| {
            let mut bgb = BindGroupBuilder::new();

            self.samplers.add_to_bg(&mut bgb);

            let uniform_buffer = uniforms::create_shader_uniform(uniforms::CreateShaderUniformArgs {
                device: &renderer.device,
                camera: graph_data.camera_manager,
                interfaces: &self.interfaces,
                ambient: self.ambient,
            });

            bgb.append_buffer(&uniform_buffer);

            let shadow_uniform_bg = bgb.build(
                &renderer.device,
                Some("shadow uniform bg"),
                &self.interfaces.shadow_uniform_bgl,
            );

            graph_data.directional_light_manager.add_to_bg(&mut bgb);

            let forward_uniform_bg = bgb.build(
                &renderer.device,
                Some("forward uniform bg"),
                &self.interfaces.forward_uniform_bgl,
            );

            graph_data.set_data(shadow_handle, Some(shadow_uniform_bg));
            graph_data.set_data(forward_handle, Some(forward_uniform_bg));
        })
    }

    pub fn add_shadow_culling_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        transparency: TransparencyType,
        shadow_index: usize,
        pre_cull_data: DataHandle<Buffer>,
        culled: DataHandle<CulledPerMaterial>,
    ) {
        let mut builder = graph.add_node(format_sso!("Shadow Culling C{} {:?}", shadow_index, transparency));

        let pre_cull_handle = self
            .gpu_culler
            .mode()
            .into_data(|| (), || builder.add_data_input(pre_cull_data));
        let cull_handle = builder.add_data_output(culled);

        builder.build(move |_pt, renderer, encoder_or_rpass, temps, ready, graph_data| {
            let encoder = encoder_or_rpass.get_encoder();

            let culling_input = pre_cull_handle.map_gpu(|handle| graph_data.get_data::<Buffer>(temps, handle).unwrap());

            let count = graph_data
                .object_manager
                .get_objects::<PbrMaterial>(transparency as u64)
                .len();

            let culled_objects = match self.gpu_culler {
                ModeData::CPU(_) => self.cpu_culler.cull(culling::cpu::CpuCullerCullArgs {
                    device: &renderer.device,
                    camera: &ready.directional_light_cameras[shadow_index],
                    interfaces: &self.interfaces,
                    objects: graph_data.object_manager,
                    transparency,
                }),
                ModeData::GPU(ref gpu_culler) => gpu_culler.cull(culling::gpu::GpuCullerCullArgs {
                    device: &renderer.device,
                    encoder,
                    interfaces: &self.interfaces,
                    camera: &ready.directional_light_cameras[shadow_index],
                    input_buffer: culling_input.into_gpu(),
                    input_count: count,
                    transparency,
                }),
            };

            let mut per_material_bgb = BindGroupBuilder::new();
            per_material_bgb.append_buffer(&culled_objects.output_buffer);

            if renderer.mode == RendererMode::GPUPowered {
                graph_data
                    .material_manager
                    .add_to_bg_gpu::<PbrMaterial>(&mut per_material_bgb);
            }

            let per_material_bg = per_material_bgb.build(&renderer.device, None, &self.interfaces.per_material_bgl);

            graph_data.set_data(
                cull_handle,
                Some(CulledPerMaterial {
                    inner: culled_objects,
                    per_material: per_material_bg,
                }),
            );
        });
    }

    pub fn add_shadow_rendering_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        transparency: TransparencyType,
        shadow_index: usize,
        shadow_uniform_bg: DataHandle<BindGroup>,
        culled: DataHandle<CulledPerMaterial>,
    ) {
        let mut builder = graph.add_node(format_sso!("{} S{} Render", transparency.to_debug_str(), shadow_index));

        let shadow_uniform_handle = builder.add_data_input(shadow_uniform_bg);
        let culled_handle = builder.add_data_input(culled);
        let shadow_output_handle = builder.add_shadow_output(shadow_index);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![],
            depth_stencil: Some(RenderPassDepthTarget {
                target: DepthHandle::Shadow(shadow_output_handle),
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        let pt_handle = builder.passthrough_ref(self);

        builder.build(move |pt, _renderer, encoder_or_pass, temps, ready, graph_data| {
            let this = pt.get(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let shadow_uniform = graph_data.get_data(temps, shadow_uniform_handle).unwrap();
            let culled = graph_data.get_data(temps, culled_handle).unwrap();

            let pipeline = match transparency {
                TransparencyType::Opaque => &this.primary_passes.shadow_opaque,
                TransparencyType::Cutout => &this.primary_passes.shadow_cutout,
                TransparencyType::Blend => unreachable!(),
            };

            graph_data.mesh_manager.buffers().bind(rpass);
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, shadow_uniform, &[]);
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

    pub fn add_culling_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        transparency: TransparencyType,
        pre_cull_data: DataHandle<Buffer>,
        culled: DataHandle<CulledPerMaterial>,
    ) {
        let mut builder = graph.add_node(format_sso!("Primary Culling {:?}", transparency));

        let pre_cull_handle = self
            .gpu_culler
            .mode()
            .into_data(|| (), || builder.add_data_input(pre_cull_data));

        let cull_handle = builder.add_data_output(culled);

        builder.build(move |_pt, renderer, encoder_or_pass, temps, _ready, graph_data| {
            let encoder = encoder_or_pass.get_encoder();

            let culling_input = pre_cull_handle.map_gpu(|handle| graph_data.get_data(temps, handle).unwrap());

            let culled_objects = match self.gpu_culler {
                ModeData::CPU(()) => self.cpu_culler.cull(CpuCullerCullArgs {
                    device: &renderer.device,
                    camera: graph_data.camera_manager,
                    interfaces: &self.interfaces,
                    objects: graph_data.object_manager,
                    transparency,
                }),
                ModeData::GPU(ref gpu_culler) => {
                    let object_count = graph_data
                        .object_manager
                        .get_objects::<PbrMaterial>(transparency as u64)
                        .len();
                    let culled = gpu_culler.cull(GpuCullerCullArgs {
                        device: &renderer.device,
                        encoder,
                        interfaces: &self.interfaces,
                        camera: graph_data.camera_manager,
                        input_buffer: culling_input.as_gpu(),
                        input_count: object_count,
                        transparency,
                    });
                    culled
                }
            };

            let mut per_material_bgb = BindGroupBuilder::new();
            per_material_bgb.append_buffer(&culled_objects.output_buffer);

            if renderer.mode == RendererMode::GPUPowered {
                graph_data
                    .material_manager
                    .add_to_bg_gpu::<PbrMaterial>(&mut per_material_bgb);
            }

            let per_material_bg = per_material_bgb.build(&renderer.device, None, &self.interfaces.per_material_bgl);

            graph_data.set_data(
                cull_handle,
                Some(CulledPerMaterial {
                    inner: culled_objects,
                    per_material: per_material_bg,
                }),
            );
        });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_prepass_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        transparency: TransparencyType,
        color: RenderTargetHandle,
        resolve: Option<RenderTargetHandle>,
        depth: RenderTargetHandle,
        forward_uniform_bg: DataHandle<BindGroup>,
        culled: DataHandle<CulledPerMaterial>,
    ) {
        let mut builder = graph.add_node(format_sso!("Primary Prepass {:?}", transparency));

        let hdr_color_handle = builder.add_render_target_output(color);
        let hdr_resolve = builder.add_optional_render_target_output(resolve);
        let hdr_depth_handle = builder.add_render_target_output(depth);

        let forward_uniform_handle = builder.add_data_input(forward_uniform_bg);
        let cull_handle = builder.add_data_input(culled);

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

        let pt_handle = builder.passthrough_ref(self);

        builder.build(move |pt, _renderer, encoder_or_pass, temps, ready, graph_data| {
            let this = pt.get(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_handle).unwrap();
            let culled = graph_data.get_data(temps, cull_handle).unwrap();

            let pipeline = match transparency {
                TransparencyType::Opaque => &this.primary_passes.depth_opaque,
                TransparencyType::Cutout => &this.primary_passes.depth_cutout,
                TransparencyType::Blend => unreachable!(),
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
    shadow_cutout: RenderPipeline,
    shadow_opaque: RenderPipeline,
    depth_cutout: RenderPipeline,
    depth_opaque: RenderPipeline,
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

        let depth_pass_args = common::depth_pass::BuildDepthPassShaderArgs {
            renderer: args.renderer,
            data_core: args.data_core,
            interfaces: args.interfaces,
            samples: SampleCount::One,
            ty: common::depth_pass::DepthPassType::Shadow,
            unclipped_depth_supported: args.unclipped_depth_supported,
        };
        let shadow_pipelines = common::depth_pass::build_depth_pass_pipeline(depth_pass_args.clone());
        let depth_pipelines =
            common::depth_pass::build_depth_pass_pipeline(common::depth_pass::BuildDepthPassShaderArgs {
                samples: args.samples,
                ty: common::depth_pass::DepthPassType::Prepass,
                ..depth_pass_args
            });

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
            shadow_cutout: shadow_pipelines.cutout,
            shadow_opaque: shadow_pipelines.opaque,
            depth_cutout: depth_pipelines.cutout,
            depth_opaque: depth_pipelines.opaque,
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

pub fn add_default_rendergraph<'node>(
    graph: &mut RenderGraph<'node>,
    ready: &ReadyData,
    pbr: &'node PbrRenderRoutine,
    skybox: Option<&'node skybox::SkyboxRoutine>,
    tonemapping: &'node tonemapping::TonemappingRoutine,
    samples: SampleCount,
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
        pbr.add_pre_cull_to_graph(graph, trans.ty, trans.pre_cull);
    }

    // Create global bind group information
    let shadow_uniform_bg = graph.add_data::<BindGroup>();
    let forward_uniform_bg = graph.add_data::<BindGroup>();
    pbr.add_uniform_bg_creation_to_graph(graph, shadow_uniform_bg, forward_uniform_bg);

    // Add shadow culling
    for trans in per_transparency_no_blend {
        for (shadow_index, &shadow_culled) in trans.shadow_cull.iter().enumerate() {
            pbr.add_shadow_culling_to_graph(graph, trans.ty, shadow_index, trans.pre_cull, shadow_culled);
        }
    }

    // Add primary culling
    for trans in &per_transparency {
        pbr.add_culling_to_graph(graph, trans.ty, trans.pre_cull, trans.cull);
    }

    // Add shadow rendering
    for trans in per_transparency_no_blend {
        for (shadow_index, &shadow_culled) in trans.shadow_cull.iter().enumerate() {
            pbr.add_shadow_rendering_to_graph(graph, trans.ty, shadow_index, shadow_uniform_bg, shadow_culled);
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
        pbr.add_prepass_to_graph(graph, trans.ty, color, resolve, depth, forward_uniform_bg, trans.cull);
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
