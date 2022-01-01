use std::borrow::Cow;

use glam::UVec2;
use rend3::{
    util::bind_merge::BindGroupBuilder, DataHandle, RenderGraph, RenderPassTarget, RenderPassTargets,
    RenderTargetHandle, Renderer, RpassTemporaryPool,
};
use wgpu::{
    BindGroup, Color, ColorTargetState, ColorWrites, Device, FragmentState, FrontFace, MultisampleState,
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

fn create_pipeline(device: &Device, interfaces: &ShaderInterfaces, output_format: TextureFormat) -> RenderPipeline {
    profiling::scope!("TonemappingPass::new");
    let blit_vert = device.create_shader_module(&ShaderModuleDescriptor {
        label: Some("tonemapping vert"),
        source: ShaderSource::Wgsl(Cow::Borrowed(
            WGSL_SHADERS
                .get_file("blit.vert.wgsl")
                .unwrap()
                .contents_utf8()
                .unwrap(),
        )),
    });

    let blit_frag = device.create_shader_module(&ShaderModuleDescriptor {
        label: Some("tonemapping frag"),
        source: ShaderSource::Wgsl(Cow::Borrowed(
            WGSL_SHADERS
                .get_file(match output_format.describe().srgb {
                    true => "blit-linear.frag.wgsl",
                    false => "blit-srgb.frag.wgsl",
                })
                .unwrap()
                .contents_utf8()
                .unwrap(),
        )),
    });

    let pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("tonemapping pass"),
        bind_group_layouts: &[&interfaces.forward_uniform_bgl, &interfaces.blit_bgl],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
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
                format: output_format,
                blend: None,
                write_mask: ColorWrites::all(),
            }],
        }),
        multiview: None,
    });

    pipeline
}

pub struct TonemappingRoutine {
    interfaces: ShaderInterfaces,
    pipeline: RenderPipeline,
    size: UVec2,
}

impl TonemappingRoutine {
    pub fn new(renderer: &Renderer, size: UVec2, output_format: TextureFormat) -> Self {
        // TODO: clean up
        let interfaces = ShaderInterfaces::new(&renderer.device, renderer.mode);

        let pipeline = create_pipeline(&renderer.device, &interfaces, output_format);

        Self {
            pipeline,
            size,
            interfaces,
        }
    }

    pub fn resize(&mut self, size: UVec2) {
        self.size = size;
    }

    pub fn add_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        src: RenderTargetHandle,
        dst: RenderTargetHandle,
        forward_uniform_bg: DataHandle<BindGroup>,
    ) {
        let mut builder = graph.add_node("Tonemapping");

        let input_handle = builder.add_render_target_input(src);
        let output_handle = builder.add_render_target_output(dst);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: None,
        });

        let forward_uniform_handle = builder.add_data_input(forward_uniform_bg);

        let pt_handle = builder.passthrough_ref(self);

        builder.build(move |pt, renderer, encoder_or_pass, temps, _ready, graph_data| {
            let this = pt.get(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_handle).unwrap();
            let hdr_color = graph_data.get_render_target(input_handle);

            profiling::scope!("tonemapping");

            let blit_src_bg = temps.add(BindGroupBuilder::new().append_texture_view(hdr_color).build(
                &renderer.device,
                Some("blit src bg"),
                &this.interfaces.blit_bgl,
            ));

            rpass.set_pipeline(&this.pipeline);
            rpass.set_bind_group(0, forward_uniform_bg, &[]);
            rpass.set_bind_group(1, blit_src_bg, &[]);
            rpass.draw(0..3, 0..1);
        });
    }
}
