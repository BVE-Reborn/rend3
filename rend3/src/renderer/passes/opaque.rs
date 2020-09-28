use crate::{
    renderer::{
        material::MAX_MATERIALS,
        shaders::{ShaderArguments, ShaderManager},
        util,
    },
    TLS,
};
use shaderc::ShaderKind;
use std::{cell::RefCell, future::Future, sync::Arc};
use switchyard::Switchyard;
use tracing_futures::Instrument;
use wgpu::{BindGroupLayout, Buffer, Device, RenderPass, RenderPipeline, ShaderModule};

pub struct OpaquePass {
    pipeline: RenderPipeline,
    vertex: Arc<ShaderModule>,
    fragment: Arc<ShaderModule>,
}
impl OpaquePass {
    pub fn new<'a, TLD>(
        device: &'a Arc<Device>,
        yard: &Switchyard<RefCell<TLD>>,
        shader_manager: &Arc<ShaderManager>,
        input_bgl: &BindGroupLayout,
        output_noindirect_bgl: &BindGroupLayout,
        material_bgl: &BindGroupLayout,
        texture_bgl: &BindGroupLayout,
        uniform_bgl: &BindGroupLayout,
    ) -> impl Future<Output = Self> + 'a
    where
        TLD: AsMut<TLS> + 'static,
    {
        let new_span = tracing::warn_span!("Creating OpaquePass");
        let new_span_guard = new_span.enter();

        let vertex = shader_manager.compile_shader(
            &yard,
            Arc::clone(device),
            ShaderArguments {
                file: String::from("rend3/shaders/opaque.vert"),
                defines: vec![],
                kind: ShaderKind::Vertex,
                debug: cfg!(debug_assertions),
            },
        );

        let fragment = shader_manager.compile_shader(
            &yard,
            Arc::clone(device),
            ShaderArguments {
                file: String::from("rend3/shaders/opaque.frag"),
                defines: vec![(String::from("MATERIAL_COUNT"), Some(MAX_MATERIALS.to_string()))],
                kind: ShaderKind::Fragment,
                debug: cfg!(debug_assertions),
            },
        );

        let layout = util::create_render_pipeline_layout(
            device,
            input_bgl,
            output_noindirect_bgl,
            material_bgl,
            texture_bgl,
            uniform_bgl,
            util::RenderPipelineType::Opaque,
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
        material_bgl: &BindGroupLayout,
        texture_bgl: &BindGroupLayout,
        uniform_bgl: &BindGroupLayout,
    ) {
        span_transfer!(_ -> update_pipeline_span, INFO, "Opaque Pass Update Pipeline");
        let layout = util::create_render_pipeline_layout(
            device,
            input_bgl,
            output_noindirect_bgl,
            material_bgl,
            texture_bgl,
            uniform_bgl,
            util::RenderPipelineType::Opaque,
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
        indirect_buffer: &'a Buffer,
        count_buffer: &'a Buffer,
        object_count: u32,
    ) {
        rpass.set_pipeline(&self.pipeline);
        rpass.multi_draw_indexed_indirect_count(indirect_buffer, 0, count_buffer, 0, object_count);
    }
}
