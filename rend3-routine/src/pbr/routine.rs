use arrayvec::ArrayVec;
use rend3::{
    format_sso,
    types::{Handedness, SampleCount},
    DataHandle, DepthHandle, ModeData, RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets,
    RenderTargetHandle, Renderer, RendererDataCore, RendererMode,
};
use wgpu::{
    BindGroup, BindGroupLayout, BlendState, Color, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Face, Features, FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, StencilState,
    TextureFormat, VertexState,
};

use crate::{
    common::{mode_safe_shader, GenericShaderInterfaces, PerMaterialInterfaces},
    culling,
    depth::DepthPipelines,
    pbr::{PbrMaterial, TransparencyType},
    vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
    CulledPerMaterial,
};

/// Render routine that renders the using PBR materials and gpu based culling.
pub struct PbrRenderRoutine {
    pub primary_passes: PrimaryPipelines,
    pub depth_pipelines: DepthPipelines<PbrMaterial>,
    pub per_material: PerMaterialInterfaces<PbrMaterial>,
}

impl PbrRenderRoutine {
    pub fn new(renderer: &Renderer, data_core: &mut RendererDataCore, interfaces: &GenericShaderInterfaces) -> Self {
        profiling::scope!("PbrRenderRoutine::new");

        data_core
            .material_manager
            .ensure_archetype::<PbrMaterial>(&renderer.device, renderer.mode);

        let unclipped_depth_supported = renderer.features.contains(Features::DEPTH_CLIP_CONTROL);

        let per_material = PerMaterialInterfaces::<PbrMaterial>::new(&renderer.device, renderer.mode);

        let depth_pipelines = DepthPipelines::<PbrMaterial>::new(
            renderer,
            data_core,
            interfaces,
            &per_material,
            unclipped_depth_supported,
        );

        let primary_passes = PrimaryPipelines::new(renderer, data_core, interfaces, &per_material);

        Self {
            depth_pipelines,
            primary_passes,
            per_material,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_forward_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        forward_uniform_bg: DataHandle<BindGroup>,
        culled: DataHandle<CulledPerMaterial>,
        samples: SampleCount,
        transparency: TransparencyType,
        color: RenderTargetHandle,
        resolve: Option<RenderTargetHandle>,
        depth: RenderTargetHandle,
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

            let pipeline = match (transparency, samples) {
                (TransparencyType::Opaque, SampleCount::One) => &this.primary_passes.forward_opaque_s1,
                (TransparencyType::Cutout, SampleCount::One) => &this.primary_passes.forward_cutout_s1,
                (TransparencyType::Blend, SampleCount::One) => &this.primary_passes.forward_blend_s1,
                (TransparencyType::Opaque, SampleCount::Four) => &this.primary_passes.forward_opaque_s4,
                (TransparencyType::Cutout, SampleCount::Four) => &this.primary_passes.forward_cutout_s4,
                (TransparencyType::Blend, SampleCount::Four) => &this.primary_passes.forward_blend_s4,
            };

            graph_data.mesh_manager.buffers().bind(rpass);

            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, forward_uniform_bg, &[]);
            rpass.set_bind_group(1, &culled.per_material, &[]);

            match culled.inner.calls {
                ModeData::CPU(ref draws) => {
                    culling::cpu::run::<PbrMaterial>(rpass, draws, graph_data.material_manager, 2)
                }
                ModeData::GPU(ref data) => {
                    rpass.set_bind_group(2, ready.d2_texture.bg.as_gpu(), &[]);
                    culling::gpu::run(rpass, data);
                }
            }
        });
    }
}

pub struct PrimaryPipelines {
    forward_blend_s1: RenderPipeline,
    forward_cutout_s1: RenderPipeline,
    forward_opaque_s1: RenderPipeline,
    forward_blend_s4: RenderPipeline,
    forward_cutout_s4: RenderPipeline,
    forward_opaque_s4: RenderPipeline,
}
impl PrimaryPipelines {
    pub fn new(
        renderer: &Renderer,
        data_core: &mut RendererDataCore,
        interfaces: &GenericShaderInterfaces,
        per_material: &PerMaterialInterfaces<PbrMaterial>,
    ) -> Self {
        profiling::scope!("PrimaryPasses::new");

        let forward_pass_vert = unsafe {
            mode_safe_shader(
                &renderer.device,
                renderer.mode,
                "forward pass vert",
                "opaque.vert.cpu.wgsl",
                "opaque.vert.gpu.spv",
            )
        };

        let forward_pass_frag = unsafe {
            mode_safe_shader(
                &renderer.device,
                renderer.mode,
                "forward pass frag",
                "opaque.frag.cpu.wgsl",
                "opaque.frag.gpu.spv",
            )
        };

        let mut bgls: ArrayVec<&BindGroupLayout, 6> = ArrayVec::new();
        bgls.push(&interfaces.forward_uniform_bgl);
        bgls.push(&per_material.bgl);
        if renderer.mode == RendererMode::GPUPowered {
            bgls.push(data_core.d2_texture_manager.gpu_bgl())
        } else {
            bgls.push(data_core.material_manager.get_bind_group_layout_cpu::<PbrMaterial>());
        }

        let pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("opaque pass"),
            bind_group_layouts: &bgls,
            push_constant_ranges: &[],
        });

        let inner = |samples, transparency| {
            build_forward_pass_inner(
                renderer,
                samples,
                transparency,
                &pll,
                &forward_pass_vert,
                &forward_pass_frag,
            )
        };

        Self {
            forward_blend_s1: inner(SampleCount::One, TransparencyType::Blend),
            forward_cutout_s1: inner(SampleCount::One, TransparencyType::Cutout),
            forward_opaque_s1: inner(SampleCount::One, TransparencyType::Opaque),
            forward_blend_s4: inner(SampleCount::Four, TransparencyType::Blend),
            forward_cutout_s4: inner(SampleCount::Four, TransparencyType::Cutout),
            forward_opaque_s4: inner(SampleCount::Four, TransparencyType::Opaque),
        }
    }
}

fn build_forward_pass_inner(
    renderer: &Renderer,
    samples: SampleCount,
    transparency: TransparencyType,
    pll: &wgpu::PipelineLayout,
    forward_pass_vert: &wgpu::ShaderModule,
    forward_pass_frag: &wgpu::ShaderModule,
) -> RenderPipeline {
    let cpu_vertex_buffers = cpu_vertex_buffers();
    let gpu_vertex_buffers = gpu_vertex_buffers();

    renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(match transparency {
            TransparencyType::Opaque => "opaque pass",
            TransparencyType::Cutout => "cutout pass",
            TransparencyType::Blend => "blend forward pass",
        }),
        layout: Some(pll),
        vertex: VertexState {
            module: forward_pass_vert,
            entry_point: "main",
            buffers: match renderer.mode {
                RendererMode::CPUPowered => &cpu_vertex_buffers,
                RendererMode::GPUPowered => &gpu_vertex_buffers,
            },
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: match renderer.handedness {
                Handedness::Left => FrontFace::Cw,
                Handedness::Right => FrontFace::Ccw,
            },
            cull_mode: Some(Face::Back),
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: matches!(transparency, TransparencyType::Opaque | TransparencyType::Cutout),
            depth_compare: match transparency {
                TransparencyType::Opaque | TransparencyType::Cutout => CompareFunction::Equal,
                TransparencyType::Blend => CompareFunction::GreaterEqual,
            },
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState {
            count: samples as u32,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: forward_pass_frag,
            entry_point: "main",
            targets: &[ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: match transparency {
                    TransparencyType::Opaque | TransparencyType::Cutout => None,
                    TransparencyType::Blend => Some(BlendState::ALPHA_BLENDING),
                },
                write_mask: ColorWrites::all(),
            }],
        }),
        multiview: None,
    })
}
