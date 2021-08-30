use std::sync::Arc;

use rend3::{
    resources::{CameraManager, InternalObject, MaterialManager, MeshBuffers},
    types::TransparencyType,
    ModeData,
};
use wgpu::{BindGroup, CommandEncoder, Device, RenderPass, RenderPipeline};

use crate::{
    common::{interfaces::ShaderInterfaces, samplers::Samplers},
    culling::{
        cpu::{CpuCuller, CpuCullerCullArgs},
        gpu::{GpuCuller, GpuCullerCullArgs},
        CulledObjectSet,
    },
};

use super::culling;

pub struct ForwardPassCullArgs<'a> {
    pub device: &'a Device,
    pub encoder: &'a mut CommandEncoder,

    pub culler: ModeData<&'a CpuCuller, &'a GpuCuller>,
    pub materials: &'a MaterialManager,

    pub interfaces: &'a ShaderInterfaces,

    pub camera: &'a CameraManager,
    pub objects: &'a [InternalObject],
}

pub struct ForwardPassPrepassArgs<'rpass, 'b> {
    pub rpass: &'b mut RenderPass<'rpass>,

    pub materials: &'rpass MaterialManager,
    pub meshes: &'rpass MeshBuffers,

    pub samplers: &'rpass Samplers,
    pub texture_bg: ModeData<(), &'rpass BindGroup>,

    pub culled_objects: &'rpass CulledObjectSet,
}

pub struct ForwardPassDrawArgs<'rpass, 'b> {
    pub rpass: &'b mut RenderPass<'rpass>,

    pub materials: &'rpass MaterialManager,
    pub meshes: &'rpass MeshBuffers,

    pub samplers: &'rpass Samplers,
    pub directional_light_bg: &'rpass BindGroup,
    pub texture_bg: ModeData<(), &'rpass BindGroup>,
    pub shader_uniform_bg: &'rpass BindGroup,

    pub culled_objects: &'rpass CulledObjectSet,
}

pub struct ForwardPass {
    depth_pipeline: Arc<RenderPipeline>,
    forward_pipeline: Arc<RenderPipeline>,
    transparency: TransparencyType,
}

impl ForwardPass {
    pub fn new(
        depth_pipeline: Arc<RenderPipeline>,
        forward_pipeline: Arc<RenderPipeline>,
        transparency: TransparencyType,
    ) -> Self {
        Self {
            depth_pipeline,
            forward_pipeline,
            transparency,
        }
    }

    pub fn cull(&self, args: ForwardPassCullArgs<'_>) -> CulledObjectSet {
        match args.culler {
            ModeData::CPU(cpu_culler) => cpu_culler.cull(CpuCullerCullArgs {
                device: args.device,
                camera: args.camera,
                interfaces: args.interfaces,
                materials: args.materials,
                objects: args.objects,
                filter: |_, m| m.transparency == self.transparency,
            }),
            ModeData::GPU(gpu_culler) => gpu_culler.cull(GpuCullerCullArgs {
                device: args.device,
                encoder: args.encoder,
                interfaces: args.interfaces,
                materials: args.materials,
                camera: args.camera,
                objects: args.objects,
                filter: |_, m| m.transparency == self.transparency,
            }),
        }
    }

    pub fn prepass<'rpass>(&'rpass self, args: ForwardPassPrepassArgs<'rpass, '_>) {
        args.meshes.bind(args.rpass);

        args.rpass.set_pipeline(&self.depth_pipeline);
        args.rpass.set_bind_group(0, &args.samplers.linear_nearest_bg, &[]);
        args.rpass.set_bind_group(1, &args.culled_objects.output_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(args.rpass, draws, args.samplers, 0, args.materials, 2),
            ModeData::GPU(ref data) => {
                args.rpass.set_bind_group(2, args.materials.gpu_get_bind_group(), &[]);
                args.rpass.set_bind_group(3, args.texture_bg.as_gpu(), &[]);
                culling::gpu::run(args.rpass, data);
            }
        }
    }

    pub fn draw<'rpass>(&'rpass self, args: ForwardPassDrawArgs<'rpass, '_>) {
        args.meshes.bind(args.rpass);

        args.rpass.set_pipeline(&self.forward_pipeline);
        args.rpass.set_bind_group(0, &args.samplers.linear_nearest_bg, &[]);
        args.rpass.set_bind_group(1, &args.culled_objects.output_bg, &[]);
        args.rpass.set_bind_group(2, args.directional_light_bg, &[]);
        args.rpass.set_bind_group(3, args.shader_uniform_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(args.rpass, draws, args.samplers, 0, args.materials, 4),
            ModeData::GPU(ref data) => {
                args.rpass.set_bind_group(4, args.materials.gpu_get_bind_group(), &[]);
                args.rpass.set_bind_group(5, args.texture_bg.as_gpu(), &[]);
                culling::gpu::run(args.rpass, data);
            }
        }
    }
}
