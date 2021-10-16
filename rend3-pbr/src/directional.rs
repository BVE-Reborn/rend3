use std::sync::Arc;

use rend3::{
    format_sso,
    resources::{CameraManager, DirectionalLightManager, MaterialManager, MeshBuffers, ObjectManager},
    ModeData,
};
use wgpu::{
    BindGroup, CommandEncoder, Device, LoadOp, Operations, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, TextureView,
};
use wgpu_profiler::GpuProfiler;

use crate::{
    common::{interfaces::ShaderInterfaces, samplers::Samplers},
    culling::{
        self,
        cpu::{CpuCuller, CpuCullerCullArgs},
        gpu::{GpuCuller, GpuCullerCullArgs, PreCulledBuffer},
        CulledObjectSet,
    },
    material::{PbrMaterial, TransparencyType},
};

pub struct DirectionalShadowPassCullShadowsArgs<'a> {
    pub device: &'a Device,
    pub profiler: &'a mut GpuProfiler,
    pub encoder: &'a mut CommandEncoder,

    pub culler: ModeData<&'a CpuCuller, &'a GpuCuller>,
    pub objects: &'a mut ObjectManager,

    pub interfaces: &'a ShaderInterfaces,

    pub lights: &'a DirectionalLightManager,

    pub culling_input_opaque: ModeData<(), &'a mut PreCulledBuffer>,
    pub culling_input_cutout: ModeData<(), &'a mut PreCulledBuffer>,
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

    pub fn cull_shadows(&self, args: DirectionalShadowPassCullShadowsArgs<'_>) -> Vec<CulledLightSet> {
        profiling::scope!("Cull Shadows");
        args.directional_light_cameras
            .iter()
            .enumerate()
            .map(|(idx, camera)| -> CulledLightSet {
                let label = format_sso!("shadow cull {}", idx);
                profiling::scope!(&label);
                // TODO: This is hella duplicated
                let opaque_culled_objects = {
                    profiling::scope!("opaque");
                    match args.culler {
                        ModeData::CPU(cpu_culler) => cpu_culler.cull(CpuCullerCullArgs {
                            device: args.device,
                            camera,
                            interfaces: args.interfaces,
                            objects: args.objects,
                            transparency: TransparencyType::Opaque,
                            sort: None,
                        }),
                        ModeData::GPU(gpu_culler) => {
                            args.profiler.begin_scope(&label, args.encoder, args.device);
                            args.profiler.begin_scope("opaque", args.encoder, args.device);
                            let culled = gpu_culler.cull(GpuCullerCullArgs {
                                device: args.device,
                                encoder: args.encoder,
                                interfaces: args.interfaces,
                                camera,
                                input_buffer: args.culling_input_opaque.as_gpu(),
                                sort: None,
                            });
                            args.profiler.end_scope(args.encoder);
                            culled
                        }
                    }
                };

                let cutout_culled_objects = {
                    profiling::scope!("cutout");
                    match args.culler {
                        ModeData::CPU(cpu_culler) => cpu_culler.cull(CpuCullerCullArgs {
                            device: args.device,
                            camera,
                            interfaces: args.interfaces,
                            objects: args.objects,
                            transparency: TransparencyType::Cutout,
                            sort: None,
                        }),
                        ModeData::GPU(gpu_culler) => {
                            args.profiler.begin_scope("cutout", args.encoder, args.device);
                            let culled = gpu_culler.cull(GpuCullerCullArgs {
                                device: args.device,
                                encoder: args.encoder,
                                interfaces: args.interfaces,
                                camera,
                                input_buffer: args.culling_input_cutout.as_gpu(),
                                sort: None,
                            });
                            args.profiler.end_scope(args.encoder);
                            args.profiler.end_scope(args.encoder);
                            culled
                        }
                    }
                };

                let shadow_texture_arc = args.lights.get_layer_view_arc(idx as _);

                CulledLightSet {
                    opaque_culled_objects,
                    cutout_culled_objects,
                    shadow_texture_arc,
                }
            })
            .collect()
    }

    pub fn draw_culled_shadows(&self, args: DirectionalShadowPassDrawCulledShadowsArgs<'_>) {
        for (idx, light) in args.culled_lights.iter().enumerate() {
            let label = format_sso!("shadow pass {}", idx);
            profiling::scope!(&label);

            args.profiler.begin_scope(&label, args.encoder, args.device);

            let mut rpass = args.encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &light.shadow_texture_arc,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            args.profiler
                .begin_scope(TransparencyType::Opaque.to_debug_str(), &mut rpass, args.device);

            args.meshes.bind(&mut rpass);

            rpass.set_pipeline(&self.opaque_pipeline);
            rpass.set_bind_group(0, &args.samplers.linear_nearest_bg, &[]);
            rpass.set_bind_group(1, &light.opaque_culled_objects.output_bg, &[]);

            match light.opaque_culled_objects.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(&mut rpass, draws, args.samplers, 0, args.materials, 2),
                ModeData::GPU(ref data) => {
                    rpass.set_bind_group(2, args.materials.get_bind_group_gpu::<PbrMaterial>(), &[]);
                    rpass.set_bind_group(3, args.texture_bg.as_gpu(), &[]);
                    culling::gpu::run(&mut rpass, data);
                }
            }

            args.profiler.end_scope(&mut rpass);
            args.profiler
                .begin_scope(TransparencyType::Cutout.to_debug_str(), &mut rpass, args.device);

            rpass.set_pipeline(&self.cutout_pipeline);
            rpass.set_bind_group(0, &args.samplers.linear_nearest_bg, &[]);
            rpass.set_bind_group(1, &light.cutout_culled_objects.output_bg, &[]);

            match light.cutout_culled_objects.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(&mut rpass, draws, args.samplers, 0, args.materials, 2),
                ModeData::GPU(ref data) => {
                    culling::gpu::run(&mut rpass, data);
                }
            }

            args.profiler.end_scope(&mut rpass);
            drop(rpass);
            args.profiler.end_scope(args.encoder);
        }
    }
}
