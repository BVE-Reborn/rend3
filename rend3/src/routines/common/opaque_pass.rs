use arrayvec::ArrayVec;
use wgpu::{
    BindGroupLayout, BlendState, ColorTargetState, ColorWrite, CompareFunction, CullMode, DepthBiasState,
    DepthStencilState, Device, FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, PushConstantRange, RenderPipeline, RenderPipelineDescriptor, ShaderFlags,
    ShaderModuleDescriptor, ShaderStage, StencilState, TextureFormat, VertexState,
};

use crate::{
    resources::MaterialManager,
    routines::{
        common::interfaces::ShaderInterfaces,
        vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
    },
    shaders::SPIRV_SHADERS,
    ModeData, RendererMode,
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
    let opaque_prepass_vert = args.device.create_shader_module(&ShaderModuleDescriptor {
        label: Some("opaque pass vert"),
        source: wgpu::util::make_spirv(
            SPIRV_SHADERS
                .get_file(match args.mode {
                    RendererMode::CPUPowered => "opaque.vert.cpu.spv",
                    RendererMode::GPUPowered => "opaque.vert.gpu.spv",
                })
                .unwrap()
                .contents(),
        ),
        flags: ShaderFlags::empty(),
    });

    let opaque_prepass_frag = args.device.create_shader_module(&ShaderModuleDescriptor {
        label: Some("opaque pass frag"),
        source: wgpu::util::make_spirv(
            SPIRV_SHADERS
                .get_file(match args.mode {
                    RendererMode::CPUPowered => "opaque.frag.cpu.spv",
                    RendererMode::GPUPowered => "opaque.frag.gpu.spv",
                })
                .unwrap()
                .contents(),
        ),
        flags: ShaderFlags::empty(),
    });

    let cpu_vertex_buffers = cpu_vertex_buffers();
    let gpu_vertex_buffers = gpu_vertex_buffers();

    let mut bgls: ArrayVec<&BindGroupLayout, 5> = ArrayVec::new();
    bgls.push(&args.interfaces.samplers_bgl);
    bgls.push(&args.interfaces.culled_object_bgl);
    bgls.push(&args.directional_light_bgl);
    bgls.push(&args.interfaces.uniform_bgl);
    bgls.push(&args.materials.get_bind_group_layout());
    match args.mode {
        RendererMode::GPUPowered => bgls.push(args.texture_bgl.as_gpu()),
        _ => {}
    };

    let mut push_constants: ArrayVec<PushConstantRange, 1> = ArrayVec::new();
    match args.mode {
        RendererMode::CPUPowered => push_constants.push(PushConstantRange {
            range: 0..4,
            stages: ShaderStage::VERTEX,
        }),
        _ => {}
    };

    let pll = args.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("depth prepass"),
        bind_group_layouts: &bgls,
        push_constant_ranges: &push_constants,
    });

    let pipeline = args.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("depth prepass"),
        layout: Some(&pll),
        vertex: VertexState {
            module: &opaque_prepass_vert,
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
            cull_mode: CullMode::Back,
            polygon_mode: PolygonMode::Fill,
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
            clamp_depth: false,
        }),
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &opaque_prepass_frag,
            entry_point: "main",
            targets: &[ColorTargetState {
                format: TextureFormat::Rgba16Float,
                alpha_blend: BlendState::REPLACE,
                color_blend: BlendState::REPLACE,
                write_mask: ColorWrite::all(),
            }],
        }),
    });

    pipeline
}
