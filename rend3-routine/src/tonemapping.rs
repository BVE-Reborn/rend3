use std::borrow::Cow;

use rend3::RpassTemporaryPool;
use wgpu::{
    BindGroup, ColorTargetState, ColorWrites, Device, FragmentState, FrontFace, MultisampleState,
    PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPass, RenderPipeline,
    RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, TextureFormat, TextureView, VertexState,
};

use crate::{common::interfaces::ShaderInterfaces, shaders::WGSL_SHADERS};

pub struct TonemappingPassNewArgs<'a> {
    pub device: &'a Device,

    pub interfaces: &'a ShaderInterfaces,

    pub output_format: TextureFormat,
}

pub struct TonemappingPassBlitArgs<'a, 'rpass> {
    pub device: &'a Device,
    pub rpass: &'a mut RenderPass<'rpass>,

    pub interfaces: &'a ShaderInterfaces,
    pub forward_uniform_bg: &'rpass BindGroup,
    pub temps: &'rpass RpassTemporaryPool<'rpass>,

    pub source: &'a TextureView,
}

pub struct TonemappingPass {
    pub pipeline: RenderPipeline,
}
impl TonemappingPass {
    pub fn new(args: TonemappingPassNewArgs<'_>) -> Self {
        profiling::scope!("TonemappingPass::new");
        let blit_vert = args.device.create_shader_module(&ShaderModuleDescriptor {
            label: Some("tonemapping vert"),
            source: ShaderSource::Wgsl(Cow::Borrowed(
                WGSL_SHADERS
                    .get_file("blit.vert.wgsl")
                    .unwrap()
                    .contents_utf8()
                    .unwrap(),
            )),
        });

        let blit_frag = args.device.create_shader_module(&ShaderModuleDescriptor {
            label: Some("tonemapping frag"),
            source: ShaderSource::Wgsl(Cow::Borrowed(
                WGSL_SHADERS
                    .get_file(match args.output_format.describe().srgb {
                        true => "blit-linear.frag.wgsl",
                        false => "blit-srgb.frag.wgsl",
                    })
                    .unwrap()
                    .contents_utf8()
                    .unwrap(),
            )),
        });

        let pll = args.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("tonemapping pass"),
            bind_group_layouts: &[&args.interfaces.forward_uniform_bgl, &args.interfaces.blit_bgl],
            push_constant_ranges: &[],
        });

        let pipeline = args.device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("tonemapping pass"),
            layout: Some(&pll),
            vertex: VertexState {
                module: &blit_vert,
                entry_point: "main",
                buffers: &[],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: FrontFace::Cw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &blit_frag,
                entry_point: "main",
                targets: &[ColorTargetState {
                    format: args.output_format,
                    blend: None,
                    write_mask: ColorWrites::all(),
                }],
            }),
            multiview: None,
        });

        Self { pipeline }
    }
}
