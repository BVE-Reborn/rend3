use crate::renderer::material::MAX_MATERIALS;
use crate::renderer::util;
use crate::{
    renderer::shaders::{ShaderArguments, ShaderManager},
    TLS,
};
use shaderc::ShaderKind;
use std::{cell::RefCell, future::Future, sync::Arc};
use switchyard::Switchyard;
use tracing_futures::Instrument;
use wgpu::{
    BindGroupLayout, CompareFunction, CullMode, DepthStencilStateDescriptor, Device, FrontFace, IndexFormat,
    PipelineLayoutDescriptor, PrimitiveTopology, ProgrammableStageDescriptor, RasterizationStateDescriptor,
    RenderPipeline, RenderPipelineDescriptor, ShaderModule, StencilStateDescriptor, TextureFormat,
    VertexStateDescriptor,
};

pub struct DepthPass {
    pipeline: RenderPipeline,
    vertex: Arc<ShaderModule>,
    fragment: Arc<ShaderModule>,
}
impl DepthPass {
    pub fn new<'a, TLD>(
        device: &'a Arc<Device>,
        yard: &Switchyard<RefCell<TLD>>,
        shader_manager: &Arc<ShaderManager>,
        input_bgl: &BindGroupLayout,
        output_bgl: &BindGroupLayout,
        material_bgl: &BindGroupLayout,
        texture_bgl: &BindGroupLayout,
        uniform_bgl: &BindGroupLayout,
    ) -> impl Future<Output = Self> + 'a
    where
        TLD: AsMut<TLS> + 'static,
    {
        let new_span = tracing::warn_span!("Creating DepthPass");
        let new_span_guard = new_span.enter();

        let vertex = shader_manager.compile_shader(
            &yard,
            Arc::clone(device),
            ShaderArguments {
                file: String::from("rend3/shaders/depth.vert"),
                defines: vec![],
                kind: ShaderKind::Vertex,
                debug: true,
            },
        );

        let fragment = shader_manager.compile_shader(
            &yard,
            Arc::clone(device),
            ShaderArguments {
                file: String::from("rend3/shaders/depth.frag"),
                defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
                kind: ShaderKind::Fragment,
                debug: true,
            },
        );

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("depth pipeline layout"),
            bind_group_layouts: &[input_bgl, output_bgl, material_bgl, texture_bgl, uniform_bgl],
            push_constant_ranges: &[],
        });

        drop(new_span_guard);

        async move {
            let vertex = vertex.await.unwrap();
            let fragment = fragment.await.unwrap();

            let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("depth pipeline"),
                layout: Some(&pipeline_layout),
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
                    cull_mode: CullMode::Back,
                    clamp_depth: false,
                    depth_bias: 0,
                    depth_bias_slope_scale: 0.0,
                    depth_bias_clamp: 0.0,
                }),
                primitive_topology: PrimitiveTopology::TriangleList,
                color_states: &[],
                depth_stencil_state: Some(DepthStencilStateDescriptor {
                    format: TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: CompareFunction::Less,
                    stencil: StencilStateDescriptor::default(),
                }),
                vertex_state: VertexStateDescriptor {
                    index_format: IndexFormat::Uint32,
                    vertex_buffers: &[create_vertex_buffer_descriptor!()],
                },
                sample_count: 1,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

            Self {
                pipeline,
                vertex,
                fragment,
            }
        }
        .instrument(new_span)
    }
}
