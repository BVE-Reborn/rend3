use std::sync::Arc;

use rend3::{
    resources::{CameraManager, DirectionalLightManager, MaterialManager, MeshBuffers, ObjectManager},
    ModeData,
};
use wgpu::{BindGroup, CommandEncoder, Device, RenderPipeline, TextureView};
use wgpu_profiler::GpuProfiler;

use crate::{
    common::{interfaces::ShaderInterfaces, samplers::Samplers},
    culling::{cpu::CpuCuller, gpu::GpuCuller, CulledObjectSet},
};

pub struct DirectionalShadowPassCullShadowsArgs<'a> {
    pub device: &'a Device,
    pub profiler: &'a mut GpuProfiler,
    pub encoder: &'a mut CommandEncoder,

    pub culler: ModeData<&'a CpuCuller, &'a GpuCuller>,
    pub objects: &'a mut ObjectManager,

    pub interfaces: &'a ShaderInterfaces,

    pub lights: &'a DirectionalLightManager,

    pub culling_input_opaque: ModeData<(), &'a mut ()>,
    pub culling_input_cutout: ModeData<(), &'a mut ()>,
    pub directional_light_cameras: &'a [CameraManager],
}

pub struct CulledLightSet {
    pub opaque_culled_objects: CulledObjectSet,
    pub cutout_culled_objects: CulledObjectSet,
    pub shadow_texture_arc: Arc<TextureView>,
}

pub struct DirectionalShadowPassDrawCulledShadowsArgs<'a> {
    pub device: &'a Device,
    pub profiler: &'a mut GpuProfiler,
    pub encoder: &'a mut CommandEncoder,

    pub materials: &'a MaterialManager,
    pub meshes: &'a MeshBuffers,

    pub samplers: &'a Samplers,
    pub texture_bg: ModeData<(), &'a BindGroup>,

    pub culled_lights: &'a [CulledLightSet],
}

pub struct DirectionalShadowPass {
    pub cutout_pipeline: Arc<RenderPipeline>,
    pub opaque_pipeline: Arc<RenderPipeline>,
}

impl DirectionalShadowPass {
    pub fn new(cutout_pipeline: Arc<RenderPipeline>, opaque_pipeline: Arc<RenderPipeline>) -> Self {
        Self {
            cutout_pipeline,
            opaque_pipeline,
        }
    }

    pub fn cull_shadows(&self, _args: DirectionalShadowPassCullShadowsArgs<'_>) -> Vec<CulledLightSet> {
        todo!()
    }

    pub fn draw_culled_shadows(&self, _args: DirectionalShadowPassDrawCulledShadowsArgs<'_>) {
        todo!()
    }
}
