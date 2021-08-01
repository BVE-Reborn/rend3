use std::sync::Arc;

use wgpu::{BindGroup, RenderPass, RenderPipeline};

use crate::{
    resources::MaterialManager,
    routines::culling::{self, CulledObjectSet},
    ModeData,
};

pub struct DepthPrepassDrawDepthPrepassArgs<'rpass, 'a> {
    pub rpass: &'a mut RenderPass<'rpass>,

    pub materials: &'rpass MaterialManager,

    pub sampler_bg: &'rpass BindGroup,
    pub texture_bg: &'rpass BindGroup,

    pub culled_objects: CulledObjectSet,
}

pub struct DepthPrepass {
    pub pipeline: Arc<RenderPipeline>,
}

impl DepthPrepass {
    pub fn new(pipeline: Arc<RenderPipeline>) -> Self {
        Self { pipeline }
    }

    pub fn draw_depth_prepass<'rpass>(&'rpass self, args: DepthPrepassDrawDepthPrepassArgs<'rpass, '_>) {
        args.rpass.set_pipeline(&self.pipeline);
        args.rpass.set_bind_group(0, args.sampler_bg, &[]);
        args.rpass.set_bind_group(1, &args.culled_objects.output_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(&mut args.rpass, &draws, args.materials, 2),
            ModeData::GPU(ref data) => {
                // TODO(ref): Figure out how to get materials or textures.
                args.rpass.set_bind_group(2, args.materials.gpu_get_bind_group(), &[]);
                args.rpass.set_bind_group(3, args.texture_bg, &[]);
                culling::gpu::run(&mut args.rpass, data);
            }
        }
    }
}
