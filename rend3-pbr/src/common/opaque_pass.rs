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
};

pub struct BuildOpaquePassShaderArgs<'a> {
    pub mode: RendererMode,
    pub device: &'a Device,

    pub interfaces: &'a ShaderInterfaces,

    pub directional_light_bgl: &'a BindGroupLayout,
    pub texture_bgl: ModeData<(), &'a BindGroupLayout>,

    pub materials: &'a MaterialManager,
}

pub fn build_opaque_pass_shader(args: BuildOpaquePassShaderArgs<'_>) -> RenderPipeline {
    let opaque_pass_vert = unsafe {
        mode_safe_shader(
            &args.device,
            args.mode,
            "opaque pass vert",
            "opaque.vert.cpu.spv",
            "opaque.vert.gpu.spv",
            false,
        )
    };

    let opaque_pass_frag = unsafe {
        mode_safe_shader(
            &args.device,
            args.mode,
            "depth pass frag",
            "opaque.frag.cpu.spv",
            "opaque.frag.gpu.spv",
            false,
        )
    };

    let cpu_vertex_buffers = cpu_vertex_buffers();
    let gpu_vertex_buffers = gpu_vertex_buffers();

    let mut bgls: ArrayVec<&BindGroupLayout, 6> = ArrayVec::new();
    bgls.push(&args.interfaces.samplers_bgl);
    bgls.push(&args.interfaces.culled_object_bgl);
    bgls.push(&args.directional_light_bgl);
    bgls.push(&args.interfaces.uniform_bgl);
    bgls.push(&args.materials.get_bind_group_layout());
    match args.mode {
        RendererMode::GPUPowered => bgls.push(args.texture_bgl.as_gpu()),
        _ => {}
    };

    let pll = args.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("opaque pass"),
        bind_group_layouts: &bgls,
        push_constant_ranges: &[],
    });

    let pipeline = args.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("opaque pass"),
        layout: Some(&pll),
        vertex: VertexState {
            module: &opaque_pass_vert,
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
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &opaque_pass_frag,
            entry_point: "main",
            targets: &[ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: None,
                write_mask: ColorWrites::all(),
            }],
        }),
    });

    pipeline
}
