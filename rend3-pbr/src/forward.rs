use std::sync::Arc;

use rend3::{
    resources::{CameraManager, MaterialManager, MeshBuffers, ObjectManager},
    ModeData,
};
use wgpu::{BindGroup, Buffer, CommandEncoder, Device, RenderPass, RenderPipeline};

use crate::{
    common::{interfaces::ShaderInterfaces, samplers::Samplers},
    culling::{
        cpu::{CpuCuller, CpuCullerCullArgs},
        gpu::{GpuCuller, GpuCullerCullArgs},
        CulledObjectSet,
    },
    material::{PbrMaterial, TransparencyType},
};

use super::culling;

pub struct ForwardPassCullArgs<'a> {
    pub device: &'a Device,
    pub encoder: &'a mut CommandEncoder,

    pub culler: ModeData<&'a CpuCuller, &'a GpuCuller>,

    pub interfaces: &'a ShaderInterfaces,

    pub camera: &'a CameraManager,
    pub objects: &'a ObjectManager,

    pub culling_input: ModeData<(), &'a Buffer>,
}

pub struct ForwardPassPrepassArgs<'rpass, 'b> {
    pub device: &'b Device,
    // pub profiler: &'b mut GpuProfiler,
    pub rpass: &'b mut RenderPass<'rpass>,

    pub bulk_bg: &'rpass BindGroup,
    pub materials: &'rpass MaterialManager,
    pub meshes: &'rpass MeshBuffers,

    pub texture_bg: ModeData<(), &'rpass BindGroup>,

    pub culled_objects: &'rpass CulledObjectSet,
}

pub struct ForwardPassDrawArgs<'rpass, 'b> {
    pub device: &'b Device,
    // pub profiler: &'b mut GpuProfiler,
    pub rpass: &'b mut RenderPass<'rpass>,

    pub bulk_bg: &'rpass BindGroup,
    pub materials: &'rpass MaterialManager,
    pub meshes: &'rpass MeshBuffers,

    pub samplers: &'rpass Samplers,
    pub directional_light_bg: &'rpass BindGroup,
    pub texture_bg: ModeData<(), &'rpass BindGroup>,
    pub shader_uniform_bg: &'rpass BindGroup,

    pub culled_objects: &'rpass CulledObjectSet,
}

pub struct ForwardPass {
    depth_pipeline: Option<Arc<RenderPipeline>>,
    forward_pipeline: Arc<RenderPipeline>,
    transparency: TransparencyType,
}

impl ForwardPass {
    pub fn new(
        depth_pipeline: Option<Arc<RenderPipeline>>,
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
                objects: args.objects,
                transparency: self.transparency,
            }),
            ModeData::GPU(gpu_culler) => {
                let object_count = args.objects.get_objects::<PbrMaterial>(self.transparency as u64).len();
                let culled = gpu_culler.cull(GpuCullerCullArgs {
                    device: args.device,
                    encoder: args.encoder,
                    interfaces: args.interfaces,
                    camera: args.camera,
                    input_buffer: args.culling_input.as_gpu(),
                    input_count: object_count,
                    transparency: self.transparency,
                });
                culled
            }
        }
    }

    pub fn prepass<'rpass>(&'rpass self, args: ForwardPassPrepassArgs<'rpass, '_>) {
        args.meshes.bind(args.rpass);

        args.rpass.set_pipeline(
            self.depth_pipeline
                .as_ref()
                .expect("prepass called on a forward pass with no depth pipeline"),
        );
        args.rpass.set_bind_group(0, &args.bulk_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(args.rpass, draws, args.materials, 1),
            ModeData::GPU(ref data) => {
                args.rpass.set_bind_group(1, args.texture_bg.as_gpu(), &[]);
                culling::gpu::run(args.rpass, data);
            }
        }
    }

    pub fn draw<'rpass>(&'rpass self, args: ForwardPassDrawArgs<'rpass, '_>) {
        args.meshes.bind(args.rpass);

        args.rpass.set_pipeline(&self.forward_pipeline);
        args.rpass.set_bind_group(0, &args.bulk_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(args.rpass, draws, args.materials, 1),
            ModeData::GPU(ref data) => {
                args.rpass.set_bind_group(1, args.texture_bg.as_gpu(), &[]);
                culling::gpu::run(args.rpass, data);
            }
        }
    }
}
