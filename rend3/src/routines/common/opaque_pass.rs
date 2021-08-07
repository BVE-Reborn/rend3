use wgpu::{Device, RenderPipeline};

use crate::{RendererMode, resources::MaterialManager};

pub struct BuildOpaquePassShaderArgs<'a> {
    pub mode: RendererMode,
    pub device: &'a Device,

    pub materials: &'a MaterialManager,
}

pub fn build_opaque_pass_shader(args: BuildOpaquePassShaderArgs<'_>) -> RenderPipeline {
    todo!()
}