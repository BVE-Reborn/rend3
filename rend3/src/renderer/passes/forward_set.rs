use crate::{
    renderer::{
        camera::Camera, passes, passes::CullingPassData, resources::RendererGlobalResources, uniforms::WrappedUniform,
    },
    Renderer, TLS,
};
use std::sync::Arc;
use wgpu::{BindGroup, BindGroupLayout, Buffer, ComputePass, Device, RenderPass};

pub struct ForwardPassSetData {
    culling_pass_data: CullingPassData,
}

pub struct ForwardPassSet {
    uniform: WrappedUniform,
    name: String,
}
impl ForwardPassSet {
    pub fn new(device: &Device, uniform_bgl: &BindGroupLayout, name: String) -> Self {
        span_transfer!(_ -> new_span, WARN, "Creating ForwardPassSet");

        let uniform = WrappedUniform::new(device, uniform_bgl);

        ForwardPassSet { uniform, name }
    }

    pub fn prepare<TLD>(
        &self,
        renderer: &Arc<Renderer<TLD>>,
        global_resources: &RendererGlobalResources,
        camera: &Camera,
        object_count: usize,
    ) -> ForwardPassSetData
    where
        TLD: AsMut<TLS> + 'static,
    {
        span_transfer!(_ -> prepare_span, WARN, "Preparing ForwardPassSet");

        self.uniform.upload(&renderer.queue, &camera);

        let culling_pass_data = renderer.culling_pass.prepare(
            &renderer.device,
            &global_resources.object_output_bgl,
            &global_resources.object_output_noindirect_bgl,
            object_count as u32,
            self.name.clone(),
        );

        ForwardPassSetData { culling_pass_data }
    }

    pub fn compute<'a>(
        &'a self,
        culling_pass: &'a passes::CullingPass,
        cpass: &mut ComputePass<'a>,
        input_bg: &'a BindGroup,
        data: &'a ForwardPassSetData,
    ) {
        span_transfer!(_ -> compute_span, WARN, "Running ForwardPassSet Compute");

        culling_pass.run(cpass, input_bg, &self.uniform.uniform_bg, &data.culling_pass_data);
    }

    pub fn depth<'a>(
        &'a self,
        depth_pass: &'a passes::DepthPass,
        rpass: &mut RenderPass<'a>,
        vertex_buffer: &'a Buffer,
        index_buffer: &'a Buffer,
        input_bg: &'a BindGroup,
        material_bg: &'a BindGroup,
        texture_bg: &'a BindGroup,
        data: &'a ForwardPassSetData,
    ) {
        span_transfer!(_ -> compute_span, WARN, "Running ForwardPassSet Depth");

        depth_pass.run(
            rpass,
            vertex_buffer,
            index_buffer,
            &data.culling_pass_data.indirect_buffer,
            &data.culling_pass_data.count_buffer,
            input_bg,
            &data.culling_pass_data.output_noindirect_bg,
            material_bg,
            texture_bg,
            &self.uniform.uniform_bg,
            data.culling_pass_data.object_count,
        );
    }

    pub fn opaque<'a>(
        &'a self,
        opaque_pass: &'a passes::OpaquePass,
        rpass: &mut RenderPass<'a>,
        data: &'a ForwardPassSetData,
    ) {
        span_transfer!(_ -> compute_span, WARN, "Running ForwardPassSet Depth");

        opaque_pass.run(
            rpass,
            &data.culling_pass_data.indirect_buffer,
            &data.culling_pass_data.count_buffer,
            data.culling_pass_data.object_count,
        );
    }
}
