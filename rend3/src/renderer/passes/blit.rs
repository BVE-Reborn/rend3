use crate::renderer::shaders::{ShaderArguments, ShaderManager};
use shaderc::ShaderKind;
use std::future::Future;
use tracing_futures::Instrument;
use wgpu::{
    BindGroup, BindGroupLayout, BlendDescriptor, ColorStateDescriptor, ColorWrite, CullMode, Device, FrontFace,
    IndexFormat, PipelineLayout, PipelineLayoutDescriptor, PolygonMode, PrimitiveTopology, ProgrammableStageDescriptor,
    RasterizationStateDescriptor, RenderPass, RenderPipeline, RenderPipelineDescriptor, ShaderModule, TextureFormat,
    VertexStateDescriptor,
};

pub struct BlitPass {
    pipeline: RenderPipeline,
}
impl BlitPass {
    pub fn new<'a>(
        device: &'a Device,
        shader_manager: &ShaderManager,
        blit_bgl: &BindGroupLayout,
        format: TextureFormat,
    ) -> impl Future<Output = Self> + 'a {
        let new_span = tracing::warn_span!("Creating BlitPass");
        let new_span_guard = new_span.enter();

        let vertex = shader_manager.compile_shader(ShaderArguments {
            file: String::from("rend3/shaders/blit.vert"),
            defines: vec![],
            kind: ShaderKind::Vertex,
            debug: cfg!(debug_assertions),
        });

        let fragment = shader_manager.compile_shader(ShaderArguments {
            file: String::from("rend3/shaders/blit.frag"),
            defines: vec![],
            kind: ShaderKind::Fragment,
            debug: cfg!(debug_assertions),
        });

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("blit pipeline layout"),
            bind_group_layouts: &[blit_bgl],
            push_constant_ranges: &[],
        });

        drop(new_span_guard);

        async move {
            let vertex = vertex.await.unwrap();
            let fragment = fragment.await.unwrap();

            let pipeline = create_blit_pipeline(device, &layout, &vertex, &fragment, format);

            Self { pipeline }
        }
        .instrument(new_span)
    }

    pub fn run<'a>(&'a self, rpass: &mut RenderPass<'a>, blit_bg: &'a BindGroup) {
        span_transfer!(_ -> run_span, WARN, "Running BlitPass");

        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, blit_bg, &[]);
        rpass.draw(0..3, 0..1);
    }
}

fn create_blit_pipeline(
    device: &Device,
    layout: &PipelineLayout,
    vertex: &ShaderModule,
    fragment: &ShaderModule,
    format: TextureFormat,
) -> RenderPipeline {
    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("blit pipeline"),
        layout: Some(&layout),
        vertex_stage: ProgrammableStageDescriptor {
            module: &vertex,
            entry_point: "main",
        },
        fragment_stage: Some(ProgrammableStageDescriptor {
            module: &fragment,
            entry_point: "main",
        }),
        rasterization_state: Some(RasterizationStateDescriptor {
            front_face: FrontFace::Ccw,
            cull_mode: CullMode::None,
            polygon_mode: PolygonMode::Fill,
            clamp_depth: false,
            depth_bias: 0,
            depth_bias_slope_scale: 0.0,
            depth_bias_clamp: 0.0,
        }),
        primitive_topology: PrimitiveTopology::TriangleList,
        color_states: &[ColorStateDescriptor {
            format,
            alpha_blend: BlendDescriptor::REPLACE,
            color_blend: BlendDescriptor::REPLACE,
            write_mask: ColorWrite::ALL,
        }],
        depth_stencil_state: None,
        vertex_state: VertexStateDescriptor {
            index_format: IndexFormat::Uint32,
            vertex_buffers: &[],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    })
}
