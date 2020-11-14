use crate::{
    list::{ShaderSourceStage, ShaderSourceType, SourceShaderDescriptor},
    renderer::{material::MAX_MATERIALS, shaders::ShaderManager, util},
};
use std::{future::Future, sync::Arc};
use tracing_futures::Instrument;
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, Device, PipelineLayout, PipelineLayoutDescriptor, RenderPass, RenderPipeline,
    ShaderModule,
};

pub enum DepthPassType {
    Depth,
    Shadow,
}

pub struct DepthPass {
    depth_pipeline: RenderPipeline,
    shadow_pipeline: RenderPipeline,
    vertex: Arc<ShaderModule>,
    fragment: Arc<ShaderModule>,
}
impl DepthPass {
    pub fn new<'a>(
        device: &'a Device,
        shader_manager: &ShaderManager,
        general_bgl: &BindGroupLayout,
        output_noindirect_bgl: &BindGroupLayout,
        texture_2d_bgl: &BindGroupLayout,
    ) -> impl Future<Output = Self> + 'a {
        let new_span = tracing::warn_span!("Creating DepthPass");
        let new_span_guard = new_span.enter();

        let vertex = shader_manager.compile_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File(String::from("rend3/shaders/depth.vert")),
            defines: vec![],
            includes: vec![],
            stage: ShaderSourceStage::Vertex,
        });

        let fragment = shader_manager.compile_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File(String::from("rend3/shaders/depth.frag")),
            defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
            includes: vec![],
            stage: ShaderSourceStage::Fragment,
        });

        let layout = create_depth_pipeline_layout(device, general_bgl, output_noindirect_bgl, texture_2d_bgl);

        drop(new_span_guard);

        async move {
            let vertex = vertex.await.unwrap();
            let fragment = fragment.await.unwrap();

            let depth_pipeline =
                util::create_render_pipeline(device, &layout, &vertex, &fragment, util::RenderPipelineType::Depth);
            let shadow_pipeline =
                util::create_render_pipeline(device, &layout, &vertex, &fragment, util::RenderPipelineType::Shadow);

            Self {
                depth_pipeline,
                shadow_pipeline,
                vertex,
                fragment,
            }
        }
        .instrument(new_span)
    }

    pub fn update_pipeline(
        &mut self,
        device: &Device,
        general_bgl: &BindGroupLayout,
        output_noindirect_bgl: &BindGroupLayout,
        texture_2d_bgl: &BindGroupLayout,
    ) {
        span_transfer!(_ -> update_pipeline_span, INFO, "Depth Pass Update Pipeline");
        let layout = create_depth_pipeline_layout(device, general_bgl, output_noindirect_bgl, texture_2d_bgl);
        let depth_pipeline = util::create_render_pipeline(
            device,
            &layout,
            &self.vertex,
            &self.fragment,
            util::RenderPipelineType::Depth,
        );
        self.depth_pipeline = depth_pipeline;
        let shadow_pipeline = util::create_render_pipeline(
            device,
            &layout,
            &self.vertex,
            &self.fragment,
            util::RenderPipelineType::Shadow,
        );
        self.shadow_pipeline = shadow_pipeline;
    }

    pub fn run<'a>(
        &'a self,
        rpass: &mut RenderPass<'a>,
        vertex_buffer: &'a Buffer,
        index_buffer: &'a Buffer,
        indirect_buffer: &'a Buffer,
        count_buffer: &'a Buffer,
        general_bg: &'a BindGroup,
        output_noindirect_bg: &'a BindGroup,
        texture_2d_bg: &'a BindGroup,
        object_count: u32,
        shadow: DepthPassType,
    ) {
        rpass.set_pipeline(match shadow {
            DepthPassType::Shadow => &self.shadow_pipeline,
            DepthPassType::Depth => &self.depth_pipeline,
        });
        rpass.set_vertex_buffer(0, vertex_buffer.slice(..));
        rpass.set_vertex_buffer(1, indirect_buffer.slice(..));
        rpass.set_index_buffer(index_buffer.slice(..));
        rpass.set_bind_group(0, &general_bg, &[]);
        rpass.set_bind_group(1, &output_noindirect_bg, &[]);
        rpass.set_bind_group(2, &texture_2d_bg, &[]);
        rpass.multi_draw_indexed_indirect_count(indirect_buffer, 0, count_buffer, 0, object_count);
    }
}

pub fn create_depth_pipeline_layout(
    device: &Device,
    input_bgl: &BindGroupLayout,
    output_noindirect_bgl: &BindGroupLayout,
    texture_2d_bgl: &BindGroupLayout,
) -> PipelineLayout {
    device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("depth pipeline layout"),
        bind_group_layouts: &[input_bgl, output_noindirect_bgl, texture_2d_bgl],
        push_constant_ranges: &[],
    })
}
