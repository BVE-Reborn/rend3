use std::sync::Arc;

use wgpu::{
    BindGroup, CommandEncoder, Device, LoadOp, Operations, RenderPass, RenderPassDepthStencilAttachmentDescriptor,
    RenderPassDescriptor, RenderPipeline, TextureView,
};

use crate::{
    resources::{CameraManager, DirectionalLightManager, InternalObject, MaterialManager},
    routines::{
        common::interfaces::ShaderInterfaces,
        culling::{
            cpu::{CpuCuller, CpuCullerCullArgs},
            gpu::{GpuCuller, GpuCullerCullArgs},
            CulledObjectSet,
        },
    },
    ModeData,
};

use super::culling;

pub struct OpaquePassCullArgs<'a> {
    pub device: &'a Device,
    pub encoder: &'a mut CommandEncoder,

    pub culler: ModeData<&'a CpuCuller, &'a GpuCuller>,
    pub materials: &'a MaterialManager,

    pub interfaces: &'a ShaderInterfaces,

    pub camera: &'a CameraManager,
    pub objects: &'a [InternalObject],
}

pub struct OpaquePassPrepassArgs<'rpass, 'b> {
    pub rpass: &'b mut RenderPass<'rpass>,

    /// TODO: only pass in manager if you actually need it
    pub materials: &'rpass MaterialManager,

    pub sampler_bg: &'rpass BindGroup,
    pub directional_light_bg: &'rpass BindGroup,
    pub texture_bg: ModeData<(), &'rpass BindGroup>,
    pub shader_uniform_bg: &'rpass BindGroup,

    pub culled_objects: &'rpass CulledObjectSet,
}

pub struct OpaquePassDrawArgs<'rpass, 'b> {
    pub rpass: &'b mut RenderPass<'rpass>,

    /// TODO: only pass in manager if you actually need it
    pub materials: &'rpass MaterialManager,

    pub sampler_bg: &'rpass BindGroup,
    pub directional_light_bg: &'rpass BindGroup,
    pub texture_bg: ModeData<(), &'rpass BindGroup>,
    pub shader_uniform_bg: &'rpass BindGroup,

    pub culled_objects: &'rpass CulledObjectSet,
}

pub struct OpaquePass {
    depth_pipeline: Arc<RenderPipeline>,
    opaque_pipeline: Arc<RenderPipeline>,
}

impl OpaquePass {
    pub fn new(depth_pipeline: Arc<RenderPipeline>, opaque_pipeline: Arc<RenderPipeline>) -> Self {
        Self { depth_pipeline, opaque_pipeline }
    }

    pub fn cull_opaque(&self, args: OpaquePassCullArgs<'_>) -> CulledObjectSet {
        match args.culler {
            ModeData::CPU(cpu_culler) => cpu_culler.cull(CpuCullerCullArgs {
                device: args.device,
                camera: &args.camera,
                interfaces: args.interfaces,
                objects: args.objects,
            }),
            ModeData::GPU(gpu_culler) => gpu_culler.cull(GpuCullerCullArgs {
                device: args.device,
                encoder: args.encoder,
                interfaces: args.interfaces,
                materials: args.materials,
                camera: &args.camera,
                objects: args.objects,
            }),
        }
    }

    pub fn prepass<'rpass>(&'rpass self, args: OpaquePassPrepassArgs<'rpass, '_>) {
        args.rpass.set_pipeline(&self.depth_pipeline);
        args.rpass.set_bind_group(0, args.sampler_bg, &[]);
        args.rpass.set_bind_group(1, &args.culled_objects.output_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(args.rpass, &draws, args.materials, 2),
            ModeData::GPU(ref data) => {
                args.rpass.set_bind_group(2, args.materials.gpu_get_bind_group(), &[]);
                args.rpass.set_bind_group(3, args.texture_bg.as_gpu(), &[]);
                culling::gpu::run(args.rpass, data);
            }
        }
    }

    pub fn draw<'rpass>(&'rpass self, args: OpaquePassDrawArgs<'rpass, '_>) {
        args.rpass.set_pipeline(&self.opaque_pipeline);
        args.rpass.set_bind_group(0, args.sampler_bg, &[]);
        args.rpass.set_bind_group(1, &args.culled_objects.output_bg, &[]);
        args.rpass.set_bind_group(2, &args.directional_light_bg, &[]);
        args.rpass.set_bind_group(3, &args.shader_uniform_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(args.rpass, &draws, args.materials, 4),
            ModeData::GPU(ref data) => {
                args.rpass.set_bind_group(4, args.materials.gpu_get_bind_group(), &[]);
                args.rpass.set_bind_group(5, args.texture_bg.as_gpu(), &[]);
                culling::gpu::run(args.rpass, data);
            }
        }
    }
}
