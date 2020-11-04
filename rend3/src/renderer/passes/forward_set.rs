use crate::{
    bind_merge::BindGroupBuilder,
    renderer::{
        camera::Camera,
        passes,
        passes::{CullingPassData, DepthPassType},
        resources::RendererGlobalResources,
        uniforms::WrappedUniform,
    },
    Renderer,
};
use std::sync::Arc;
use wgpu::{BindGroup, BindGroupEntry, BindGroupLayout, Buffer, ComputePass, Device, RenderPass};

pub struct ForwardPassSetData {
    culling_pass_data: CullingPassData,
    object_output_noindirect_bg: BindGroup,
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

    pub fn prepare<TLD: 'static>(
        &self,
        renderer: &Arc<Renderer<TLD>>,
        global_resources: &RendererGlobalResources,
        camera: &Camera,
        object_count: usize,
    ) -> ForwardPassSetData {
        span_transfer!(_ -> prepare_span, WARN, "Preparing ForwardPassSet");

        let mut object_output_noindirect_bgb =
            BindGroupBuilder::new(Some(String::from("object output noindirect bgb")));

        let culling_pass_data = renderer.culling_pass.prepare(
            &renderer.device,
            &global_resources.prefix_sum_bgl,
            &global_resources.pre_cull_bgl,
            &global_resources.object_output_bgl,
            object_count as u32,
            self.name.clone(),
        );

        object_output_noindirect_bgb.append(BindGroupEntry {
            binding: 0,
            resource: culling_pass_data.output_buffer.as_entire_binding(),
        });

        self.uniform
            .upload(&renderer.queue, &camera, &mut object_output_noindirect_bgb);

        let object_output_noindirect_bg =
            object_output_noindirect_bgb.build(&renderer.device, &global_resources.object_output_noindirect_bgl);

        ForwardPassSetData {
            culling_pass_data,
            object_output_noindirect_bg,
        }
    }

    pub fn compute<'a>(
        &'a self,
        culling_pass: &'a passes::CullingPass,
        cpass: &mut ComputePass<'a>,
        general_bg: &'a BindGroup,
        data: &'a ForwardPassSetData,
    ) {
        span_transfer!(_ -> compute_span, WARN, "Running ForwardPassSet Compute");

        culling_pass.run(cpass, general_bg, &self.uniform.uniform_bg, &data.culling_pass_data);
    }

    pub fn render<'a>(
        &'a self,
        depth_pass: &'a passes::DepthPass,
        skybox_pass: &'a passes::SkyboxPass,
        opaque_pass: &'a passes::OpaquePass,
        rpass: &mut RenderPass<'a>,
        vertex_buffer: &'a Buffer,
        index_buffer: &'a Buffer,
        general_bg: &'a BindGroup,
        texture_2d_bg: &'a BindGroup,
        texture_cube_bg: &'a BindGroup,
        texture_internal_bg: &'a BindGroup,
        data: &'a ForwardPassSetData,
        background_texture: Option<u32>,
    ) {
        span_transfer!(_ -> compute_span, WARN, "Running ForwardPassSet Render");

        depth_pass.run(
            rpass,
            vertex_buffer,
            index_buffer,
            &data.culling_pass_data.indirect_buffer,
            &data.culling_pass_data.count_buffer,
            general_bg,
            &data.object_output_noindirect_bg,
            texture_2d_bg,
            data.culling_pass_data.object_count,
            DepthPassType::Depth,
        );

        if let Some(background_texture) = background_texture {
            skybox_pass.run(
                rpass,
                general_bg,
                &data.object_output_noindirect_bg,
                texture_cube_bg,
                background_texture,
            );
        }

        opaque_pass.run(
            rpass,
            vertex_buffer,
            index_buffer,
            &data.culling_pass_data.indirect_buffer,
            &data.culling_pass_data.count_buffer,
            general_bg,
            &data.object_output_noindirect_bg,
            texture_2d_bg,
            texture_internal_bg,
            data.culling_pass_data.object_count,
        );
    }
}
