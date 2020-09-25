use crate::{
    renderer::{camera::Camera, passes::CullingPassData, resources::RendererGlobalResources, uniforms::WrappedUniform},
    Renderer, TLS,
};
use std::sync::Arc;
use wgpu::{BindGroup, BindGroupLayout, ComputePass, Device};

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
            object_count as u32,
            self.name.clone(),
        );

        ForwardPassSetData { culling_pass_data }
    }

    pub fn compute<'a, TLD>(
        &'a self,
        renderer: &'a Arc<Renderer<TLD>>,
        compute_pass: &mut ComputePass<'a>,
        input_bg: &'a BindGroup,
        data: &'a ForwardPassSetData,
    ) where
        TLD: AsMut<TLS> + 'static,
    {
        span_transfer!(_ -> compute_span, WARN, "Running ForwardPassSet Compute");

        renderer.culling_pass.run(
            compute_pass,
            input_bg,
            &self.uniform.uniform_bg,
            &data.culling_pass_data,
        );
    }
}
