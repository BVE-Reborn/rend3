use wgpu::{
    BindGroup, BlendState, Color, ColorTargetState, ColorWrite, CommandEncoder, CullMode, Device, FragmentState,
    FrontFace, LoadOp, MultisampleState, Operations, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    PrimitiveTopology, RenderPassColorAttachmentDescriptor, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, ShaderFlags, ShaderModuleDescriptor, TextureFormat, TextureView, VertexState,
};

use crate::{
    routines::common::interfaces::ShaderInterfaces, shaders::SPIRV_SHADERS, util::bind_merge::BindGroupBuilder,
};

pub struct TonemappingPassNewArgs<'a> {
    pub device: &'a Device,

    pub interfaces: &'a ShaderInterfaces,
}

pub struct TonemappingPassBlitArgs<'a> {
    pub device: &'a Device,
    pub encoder: &'a mut CommandEncoder,

    pub interfaces: &'a ShaderInterfaces,

    pub samplers_bg: &'a BindGroup,

    pub source: &'a TextureView,
    pub target: &'a TextureView,
}

pub struct TonemappingPass {
    pipeline: RenderPipeline,
}
impl TonemappingPass {
    pub fn new(args: TonemappingPassNewArgs<'_>) -> Self {
        let blit_vert = args.device.create_shader_module(&ShaderModuleDescriptor {
            label: Some("tonemapping vert"),
            source: wgpu::util::make_spirv(SPIRV_SHADERS.get_file("blit.vert.spv").unwrap().contents()),
            flags: ShaderFlags::empty(),
        });

        let blit_frag = args.device.create_shader_module(&ShaderModuleDescriptor {
            label: Some("tonemapping frag"),
            source: wgpu::util::make_spirv(SPIRV_SHADERS.get_file("blit.frag.spv").unwrap().contents()),
            flags: ShaderFlags::empty(),
        });

        let pll = args.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("tonemapping pass"),
            bind_group_layouts: &[&args.interfaces.samplers_bgl, &args.interfaces.blit_bgl],
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
                cull_mode: CullMode::None,
                polygon_mode: PolygonMode::Fill,
            },
            depth_stencil: None,
            multisample: MultisampleState::default(),
            fragment: Some(FragmentState {
                module: &blit_frag,
                entry_point: "main",
                targets: &[ColorTargetState {
                    format: TextureFormat::Rgba8Unorm,
                    alpha_blend: BlendState::REPLACE,
                    color_blend: BlendState::REPLACE,
                    write_mask: ColorWrite::all(),
                }],
            }),
        });

        Self { pipeline }
    }

    pub fn blit(&self, args: TonemappingPassBlitArgs<'_>) {
        let blit_src_bg = BindGroupBuilder::new(Some("blit src bg"))
            .with_texture_view(args.source)
            .build(args.device, &args.interfaces.blit_bgl);

        let mut rpass = args.encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("tonemapping pass"),
            color_attachments: &[RenderPassColorAttachmentDescriptor {
                attachment: args.target,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &args.samplers_bg, &[]);
        rpass.set_bind_group(1, &blit_src_bg, &[]);
        rpass.draw(0..3, 0..1);
    }
}
