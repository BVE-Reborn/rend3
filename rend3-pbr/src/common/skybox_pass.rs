use rend3::RendererMode;
use wgpu::{
    ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState, Device, Face, FragmentState,
    FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology,
    RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, StencilState, TextureFormat, VertexState,
};

use crate::{common::interfaces::ShaderInterfaces, shaders::SPIRV_SHADERS};

pub struct BuildSkyboxShaderArgs<'a> {
    pub mode: RendererMode,
    pub device: &'a Device,

    pub interfaces: &'a ShaderInterfaces,
}

pub fn build_skybox_shader(args: BuildSkyboxShaderArgs<'_>) -> RenderPipeline {
    let skybox_pass_vert = args.device.create_shader_module(&ShaderModuleDescriptor {
        label: Some("skybox vert"),
        source: wgpu::util::make_spirv(SPIRV_SHADERS.get_file("skybox.vert.spv").unwrap().contents()),
    });
    let skybox_pass_frag = args.device.create_shader_module(&ShaderModuleDescriptor {
        label: Some("skybox frag"),
        source: wgpu::util::make_spirv(SPIRV_SHADERS.get_file("skybox.frag.spv").unwrap().contents()),
    });

    let pll = args.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("skybox pass"),
        bind_group_layouts: &[
            &args.interfaces.samplers_bgl,
            &args.interfaces.skybox_bgl,
            &args.interfaces.uniform_bgl,
        ],
        push_constant_ranges: &[],
    });

    args.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("skybox pass"),
        layout: Some(&pll),
        vertex: VertexState {
            module: &skybox_pass_vert,
            entry_point: "main",
            buffers: &[],
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
            module: &skybox_pass_frag,
            entry_point: "main",
            targets: &[ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend: None,
                write_mask: ColorWrites::all(),
            }],
        }),
    })
}
