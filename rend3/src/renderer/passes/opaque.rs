use crate::renderer::{
    material::MAX_MATERIALS,
    shaders::{ShaderArguments, ShaderManager},
    util,
};
use shaderc::ShaderKind;
use std::{future::Future, sync::Arc};
use tracing_futures::Instrument;
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, Device, PipelineLayout, PipelineLayoutDescriptor, RenderPass, RenderPipeline,
    ShaderModule,
};

pub struct OpaquePass {
    pipeline: RenderPipeline,
    vertex: Arc<ShaderModule>,
    fragment: Arc<ShaderModule>,
}
impl OpaquePass {
    pub fn new<'a>(
        device: &'a Device,
        shader_manager: &ShaderManager,
        input_bgl: &BindGroupLayout,
        output_noindirect_bgl: &BindGroupLayout,
        texture_2d_bgl: &BindGroupLayout,
        texture_internal_bgl: &BindGroupLayout,
    ) -> impl Future<Output = Self> + 'a {
        let new_span = tracing::warn_span!("Creating OpaquePass");
        let new_span_guard = new_span.enter();

        let vertex = shader_manager.compile_shader(ShaderArguments {
            file: String::from("rend3/shaders/opaque.vert"),
            defines: vec![],
            kind: ShaderKind::Vertex,
            debug: cfg!(debug_assertions),
        });

        let fragment = shader_manager.compile_shader(ShaderArguments {
            file: String::from("rend3/shaders/opaque.frag"),
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
            kind: ShaderKind::Fragment,
            debug: cfg!(debug_assertions),
        });

        let layout = create_opaque_pipeline_layout(
            device,
            input_bgl,
            output_noindirect_bgl,
            texture_2d_bgl,
            texture_internal_bgl,
        );

        drop(new_span_guard);

        async move {
            let vertex = vertex.await.unwrap();
            let fragment = fragment.await.unwrap();

            let pipeline =
                util::create_render_pipeline(device, &layout, &vertex, &fragment, util::RenderPipelineType::Opaque);

            Self {
                pipeline,
                vertex,
                fragment,
            }
        }
        .instrument(new_span)
    }

    pub fn update_pipeline(
        &mut self,
        device: &Device,
        input_bgl: &BindGroupLayout,
        output_noindirect_bgl: &BindGroupLayout,
        texture_2d_bgl: &BindGroupLayout,
        texture_internal_bgl: &BindGroupLayout,
    ) {
        span_transfer!(_ -> update_pipeline_span, INFO, "Opaque Pass Update Pipeline");
        let layout = create_opaque_pipeline_layout(
            device,
            input_bgl,
            output_noindirect_bgl,
            texture_2d_bgl,
            texture_internal_bgl,
        );
        let pipeline = util::create_render_pipeline(
            device,
            &layout,
            &self.vertex,
            &self.fragment,
            util::RenderPipelineType::Opaque,
        );
        self.pipeline = pipeline;
    }

    pub fn run<'a>(
        &'a self,
        rpass: &mut RenderPass<'a>,
        vertex_buffer: &'a Buffer,
        index_buffer: &'a Buffer,
        indirect_buffer: &'a Buffer,
        count_buffer: &'a Buffer,
        input_bg: &'a BindGroup,
        output_noindirect_bg: &'a BindGroup,
        texture_2d_bg: &'a BindGroup,
        texture_internal_bg: &'a BindGroup,
        object_count: u32,
    ) {
        rpass.set_pipeline(&self.pipeline);
        rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
        rpass.set_index_buffer(index_buffer.slice(..));
        rpass.set_bind_group(0, &input_bg, &[]);
        rpass.set_bind_group(1, &output_noindirect_bg, &[]);
        rpass.set_bind_group(2, &texture_2d_bg, &[]);
        rpass.set_bind_group(3, &texture_internal_bg, &[]);
        rpass.multi_draw_indexed_indirect_count(indirect_buffer, 0, count_buffer, 0, object_count);
    }
}

pub fn create_opaque_pipeline_layout(
    device: &Device,
    input_bgl: &BindGroupLayout,
    output_noindirect_bgl: &BindGroupLayout,
    texture_2d_bgl: &BindGroupLayout,
    texture_internal_bgl: &BindGroupLayout,
) -> PipelineLayout {
    device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("opaque pipeline layout"),
        bind_group_layouts: &[input_bgl, output_noindirect_bgl, texture_2d_bgl, texture_internal_bgl],
        push_constant_ranges: &[],
    })
}
