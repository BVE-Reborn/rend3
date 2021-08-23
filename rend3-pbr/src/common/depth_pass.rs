use arrayvec::ArrayVec;
use rend3::{resources::MaterialManager, ModeData, RendererMode};
use wgpu::{
    BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState, Device, Face,
    FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, StencilState, TextureFormat, VertexState,
};

use crate::{
    common::{interfaces::ShaderInterfaces, shaders::mode_safe_shader},
    vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
    SampleCount,
};

pub struct BuildDepthPassShaderArgs<'a> {
    pub mode: RendererMode,
    pub device: &'a Device,

    pub interfaces: &'a ShaderInterfaces,
    pub texture_bgl: ModeData<(), &'a BindGroupLayout>,

    pub materials: &'a MaterialManager,

    pub samples: SampleCount,
    /// TODO: How could this be better expressed
    pub include_color: bool,
}

pub fn build_depth_pass_shader(args: BuildDepthPassShaderArgs) -> RenderPipeline {
    let depth_prepass_vert = unsafe {
        mode_safe_shader(
            args.device,
            args.mode,
            "depth pass vert",
            "depth.vert.cpu.spv",
            "depth.vert.gpu.spv",
            false,
        )
    };

    let depth_prepass_frag = unsafe {
        mode_safe_shader(
            args.device,
            args.mode,
            "depth pass frag",
            "depth.frag.cpu.spv",
            "depth.frag.gpu.spv",
            false,
        )
    };

    let cpu_vertex_buffers = cpu_vertex_buffers();
    let gpu_vertex_buffers = gpu_vertex_buffers();

    let mut bgls: ArrayVec<&BindGroupLayout, 4> = ArrayVec::new();
    bgls.push(&args.interfaces.samplers_bgl);
    bgls.push(&args.interfaces.culled_object_bgl);
    bgls.push(args.materials.get_bind_group_layout());
    if args.mode == RendererMode::GPUPowered {
        bgls.push(args.texture_bgl.as_gpu())
    }

    let pll = args.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("depth prepass"),
        bind_group_layouts: &bgls,
        push_constant_ranges: &[],
    });

    let color_state = [ColorTargetState {
        format: TextureFormat::Rgba16Float,
        blend: None,
        write_mask: ColorWrites::empty(),
    }];

    args.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("depth prepass"),
        layout: Some(&pll),
        vertex: VertexState {
            module: &depth_prepass_vert,
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
            cull_mode: Some(Face::Back),
            clamp_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: if args.include_color {
                CompareFunction::GreaterEqual
            } else {
                CompareFunction::LessEqual
            },
            stencil: StencilState::default(),
            bias: if args.include_color {
                DepthBiasState::default()
            } else {
                DepthBiasState {
                    constant: 2,
                    slope_scale: 2.0,
                    clamp: 0.0,
                }
            },
        }),
        multisample: MultisampleState {
            count: args.samples as u32,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: &depth_prepass_frag,
            entry_point: "main",
            targets: if args.include_color { &color_state } else { &[] },
        }),
    })
}
