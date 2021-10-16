use std::sync::Arc;

use rend3::{
    format_sso,
    resources::{CameraManager, MaterialManager, MeshBuffers, ObjectManager},
    ModeData,
};
use wgpu::{BindGroup, Buffer, CommandEncoder, Device, RenderPass, RenderPipeline};
use wgpu_profiler::GpuProfiler;

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
    pub profiler: &'a mut GpuProfiler,
    pub encoder: &'a mut CommandEncoder,

    pub culler: ModeData<&'a CpuCuller, &'a GpuCuller>,

    pub interfaces: &'a ShaderInterfaces,

    pub camera: &'a CameraManager,
    pub objects: &'a ObjectManager,

    pub culling_input: ModeData<(), &'a Buffer>,
}

pub struct ForwardPassPrepassArgs<'rpass, 'b> {
    pub device: &'b Device,
    pub profiler: &'b mut GpuProfiler,
    pub rpass: &'b mut RenderPass<'rpass>,

    pub materials: &'rpass MaterialManager,
    pub meshes: &'rpass MeshBuffers,

    pub samplers: &'rpass Samplers,
    pub texture_bg: ModeData<(), &'rpass BindGroup>,

    pub culled_objects: &'rpass CulledObjectSet,
}

pub struct ForwardPassDrawArgs<'rpass, 'b> {
    pub device: &'b Device,
    pub profiler: &'b mut GpuProfiler,
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
        let label = format_sso!("forward cull {}", self.transparency.to_debug_str());
        profiling::scope!(&label);

        match args.culler {
            ModeData::CPU(cpu_culler) => cpu_culler.cull(CpuCullerCullArgs {
                device: args.device,
                camera: args.camera,
                interfaces: args.interfaces,
                objects: args.objects,
                transparency: self.transparency,
            }),
            ModeData::GPU(gpu_culler) => {
                args.profiler.begin_scope(&label, args.encoder, args.device);
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
                args.profiler.end_scope(args.encoder);
                culled
            }
        }
    }

    pub fn prepass<'rpass>(&'rpass self, args: ForwardPassPrepassArgs<'rpass, '_>) {
        let label = self.transparency.to_debug_str();
        profiling::scope!(label);
        args.profiler.begin_scope(label, args.rpass, args.device);

        args.meshes.bind(args.rpass);

        args.rpass.set_pipeline(
            self.depth_pipeline
                .as_ref()
                .expect("prepass called on a forward pass with no depth pipeline"),
        );
        args.rpass.set_bind_group(0, &args.samplers.linear_nearest_bg, &[]);
        args.rpass.set_bind_group(1, &args.culled_objects.output_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(args.rpass, draws, args.samplers, 0, args.materials, 2),
            ModeData::GPU(ref data) => {
                args.rpass
                    .set_bind_group(2, args.materials.get_bind_group_gpu::<PbrMaterial>(), &[]);
                args.rpass.set_bind_group(3, args.texture_bg.as_gpu(), &[]);
                culling::gpu::run(args.rpass, data);
            }
        }
        args.profiler.end_scope(args.rpass);
    }

    pub fn draw<'rpass>(&'rpass self, args: ForwardPassDrawArgs<'rpass, '_>) {
        let label = self.transparency.to_debug_str();
        profiling::scope!(label);
        args.profiler.begin_scope(label, args.rpass, args.device);

        args.meshes.bind(args.rpass);

        args.rpass.set_pipeline(&self.forward_pipeline);
        args.rpass.set_bind_group(0, &args.samplers.linear_nearest_bg, &[]);
        args.rpass.set_bind_group(1, &args.culled_objects.output_bg, &[]);
        args.rpass.set_bind_group(2, args.directional_light_bg, &[]);
        args.rpass.set_bind_group(3, args.shader_uniform_bg, &[]);

        match args.culled_objects.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(args.rpass, draws, args.samplers, 0, args.materials, 4),
            ModeData::GPU(ref data) => {
                args.rpass
                    .set_bind_group(4, args.materials.get_bind_group_gpu::<PbrMaterial>(), &[]);
                args.rpass.set_bind_group(5, args.texture_bg.as_gpu(), &[]);
                culling::gpu::run(args.rpass, data);
            }
        }

        args.profiler.end_scope(args.rpass);
    }
}
