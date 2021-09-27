use std::sync::Arc;

use arrayvec::ArrayVec;
use rend3::{resources::MaterialManager, ModeData, RendererMode};
use wgpu::{
    BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState, Device, Face,
    FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, StencilState, TextureFormat, VertexState,
};

use crate::{
    common::{interfaces::ShaderInterfaces, shaders::mode_safe_shader},
    material::PbrMaterial,
    vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
    SampleCount,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthPassType {
    Shadow,
    Prepass,
}

pub struct BuildDepthPassShaderArgs<'a> {
    pub mode: RendererMode,
    pub device: &'a Device,

    pub interfaces: &'a ShaderInterfaces,
    pub texture_bgl: ModeData<(), &'a BindGroupLayout>,

    pub materials: &'a MaterialManager,

    pub samples: SampleCount,
    pub ty: DepthPassType,
}

#[derive(Clone)]
pub struct DepthPassPipelines {
    pub cutout: Arc<RenderPipeline>,
    pub opaque: Arc<RenderPipeline>,
}

pub fn build_depth_pass_shader(args: BuildDepthPassShaderArgs) -> DepthPassPipelines {
    let depth_vert = unsafe {
        mode_safe_shader(
            args.device,
            args.mode,
            "depth pass vert",
            "depth.vert.cpu.spv",
            "depth.vert.gpu.spv",
            false,
        )
    };

    let depth_opaque_frag = unsafe {
        mode_safe_shader(
            args.device,
            args.mode,
            "depth pass opaque frag",
            "depth-opaque.frag.cpu.spv",
            "depth-opaque.frag.gpu.spv",
            false,
        )
    };

    let depth_cutout_frag = unsafe {
        mode_safe_shader(
            args.device,
            args.mode,
            "depth pass cutout frag",
            "depth-cutout.frag.cpu.spv",
            "depth-cutout.frag.gpu.spv",
            false,
        )
    };

    let mut bgls: ArrayVec<&BindGroupLayout, 4> = ArrayVec::new();
    bgls.push(&args.interfaces.samplers_bgl);
    bgls.push(&args.interfaces.culled_object_bgl);
    bgls.push(args.materials.get_bind_group_layout::<PbrMaterial>());
    if args.mode == RendererMode::GPUPowered {
        bgls.push(args.texture_bgl.as_gpu())
    }

    let pll = args.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("depth prepass"),
        bind_group_layouts: &bgls,
        push_constant_ranges: &[],
    });

    DepthPassPipelines {
        opaque: Arc::new(create_depth_inner(
            &args,
            &pll,
            &depth_vert,
            &depth_opaque_frag,
            "depth opaque prepass",
        )),
        cutout: Arc::new(create_depth_inner(
            &args,
            &pll,
            &depth_vert,
            &depth_cutout_frag,
            "depth cutout prepass",
        )),
    }
}

fn create_depth_inner(
    args: &BuildDepthPassShaderArgs,
    pll: &wgpu::PipelineLayout,
    vert: &wgpu::ShaderModule,
    frag: &wgpu::ShaderModule,
    name: &str,
) -> RenderPipeline {
    let color_state = [ColorTargetState {
        format: TextureFormat::Rgba16Float,
        blend: None,
        write_mask: ColorWrites::empty(),
    }];
    let cpu_vertex_buffers = cpu_vertex_buffers();
    let gpu_vertex_buffers = gpu_vertex_buffers();
    args.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(name),
        layout: Some(pll),
        vertex: VertexState {
            module: vert,
            entry_point: "main",
            buffers: match args.mode {
                RendererMode::CPUPowered => &cpu_vertex_buffers,
                RendererMode::GPUPowered => &gpu_vertex_buffers,
            },
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Cw,
            cull_mode: Some(match args.ty {
                DepthPassType::Shadow => Face::Front,
                DepthPassType::Prepass => Face::Back,
            }),
            clamp_depth: matches!(args.ty, DepthPassType::Shadow),
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: match args.ty {
                DepthPassType::Shadow => CompareFunction::LessEqual,
                DepthPassType::Prepass => CompareFunction::GreaterEqual,
            },
            stencil: StencilState::default(),
            bias: match args.ty {
                DepthPassType::Prepass => DepthBiasState::default(),
                DepthPassType::Shadow => DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            },
        }),
        multisample: MultisampleState {
            count: args.samples as u32,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: frag,
            entry_point: "main",
            targets: match args.ty {
                DepthPassType::Prepass => &color_state,
                DepthPassType::Shadow => &[],
            },
        }),
    })
}
