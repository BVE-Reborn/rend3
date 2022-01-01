use arrayvec::ArrayVec;
#[allow(unused_imports)]
use rend3::format_sso;
use rend3::{
    types::{Handedness, SampleCount},
    Renderer, RendererDataCore, RendererMode,
};
use wgpu::{
    BindGroupLayout, BlendState, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState,
    Face, FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, StencilState, TextureFormat, VertexState,
};

use crate::{
    common::{interfaces::ShaderInterfaces, shaders::mode_safe_shader},
    material::{PbrMaterial, TransparencyType},
    vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
};

#[derive(Clone)]
pub struct BuildForwardPassShaderArgs<'a> {
    pub renderer: &'a Renderer,
    pub data_core: &'a RendererDataCore,

    pub interfaces: &'a ShaderInterfaces,

    pub samples: SampleCount,
    pub transparency: TransparencyType,
}

pub fn build_forward_pass_pipeline(args: BuildForwardPassShaderArgs<'_>) -> RenderPipeline {
    profiling::scope!(
        "build forward pass pipeline",
        &format_sso!("{:?}:samp {:?}:bake {:?}", args.transparency, args.samples, args.baking)
    );
    let forward_pass_vert = unsafe {
        mode_safe_shader(
            &args.renderer.device,
            args.renderer.mode,
            "forward pass vert",
            "opaque.vert.cpu.wgsl",
            "opaque.vert.gpu.spv",
        )
    };

    let forward_pass_frag = unsafe {
        mode_safe_shader(
            &args.renderer.device,
            args.renderer.mode,
            "forward pass frag",
            "opaque.frag.cpu.wgsl",
            "opaque.frag.gpu.spv",
        )
    };

    let mut bgls: ArrayVec<&BindGroupLayout, 6> = ArrayVec::new();
    bgls.push(&args.interfaces.forward_uniform_bgl);
    bgls.push(&args.interfaces.per_material_bgl);
    if args.renderer.mode == RendererMode::GPUPowered {
        bgls.push(args.data_core.d2_texture_manager.gpu_bgl())
    } else {
        bgls.push(
            args.data_core
                .material_manager
                .get_bind_group_layout_cpu::<PbrMaterial>(),
        );
    }

    let pll = args.renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("opaque pass"),
        bind_group_layouts: &bgls,
        push_constant_ranges: &[],
    });

    build_forward_pass_inner(args, pll, forward_pass_vert, forward_pass_frag)
}

fn build_forward_pass_inner(
    args: BuildForwardPassShaderArgs,
    pll: wgpu::PipelineLayout,
    forward_pass_vert: wgpu::ShaderModule,
    forward_pass_frag: wgpu::ShaderModule,
) -> RenderPipeline {
    let cpu_vertex_buffers = cpu_vertex_buffers();
    let gpu_vertex_buffers = gpu_vertex_buffers();

    args.renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(match args.transparency {
            TransparencyType::Opaque => "opaque pass",
            TransparencyType::Cutout => "cutout pass",
            TransparencyType::Blend => "blend forward pass",
        }),
        layout: Some(&pll),
        vertex: VertexState {
            module: &forward_pass_vert,
            entry_point: "main",
            buffers: match args.renderer.mode {
                RendererMode::CPUPowered => &cpu_vertex_buffers,
                RendererMode::GPUPowered => &gpu_vertex_buffers,
            },
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: match args.renderer.handedness {
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
            depth_write_enabled: matches!(args.transparency, TransparencyType::Opaque | TransparencyType::Cutout),
            depth_compare: match args.transparency {
                TransparencyType::Opaque | TransparencyType::Cutout => CompareFunction::Equal,
                TransparencyType::Blend => CompareFunction::GreaterEqual,
            },
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState {
            count: args.samples as u32,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: &forward_pass_frag,
            entry_point: "main",
            targets: &[ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: match args.transparency {
                    TransparencyType::Opaque | TransparencyType::Cutout => None,
                    TransparencyType::Blend => Some(BlendState::ALPHA_BLENDING),
                },
                write_mask: ColorWrites::all(),
            }],
        }),
        multiview: None,
    })
}
