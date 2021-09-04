use std::sync::Arc;

use rend3::{
    format_sso,
    resources::{DirectionalLightManager, InternalObject, MaterialManager, MeshBuffers},
    types::TransparencyType,
    ModeData,
};
use wgpu::{
    BindGroup, CommandEncoder, Device, LoadOp, Operations, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, TextureView,
};

use crate::{
    common::{interfaces::ShaderInterfaces, samplers::Samplers},
    culling::{
        cpu::{CpuCuller, CpuCullerCullArgs},
        gpu::{GpuCuller, GpuCullerCullArgs},
        CulledObjectSet,
    },
};

use super::culling;

pub struct DirectionalShadowPassCullShadowsArgs<'a> {
    pub device: &'a Device,
    pub encoder: &'a mut CommandEncoder,

    pub culler: ModeData<&'a CpuCuller, &'a GpuCuller>,
    pub materials: &'a MaterialManager,

    pub interfaces: &'a ShaderInterfaces,

    pub lights: &'a DirectionalLightManager,
    pub objects: &'a [InternalObject],
}

pub struct CulledLightSet {
    pub opaque_culled_objects: CulledObjectSet,
    pub cutout_culled_objects: CulledObjectSet,
    pub shadow_texture_arc: Arc<TextureView>,
}

pub struct DirectionalShadowPassDrawCulledShadowsArgs<'a> {
    pub encoder: &'a mut CommandEncoder,

    pub materials: &'a MaterialManager,
    pub meshes: &'a MeshBuffers,

    pub samplers: &'a Samplers,
    pub texture_bg: ModeData<(), &'a BindGroup>,

    pub culled_lights: &'a [CulledLightSet],
}

pub struct DirectionalShadowPass {
    cutout_pipeline: Arc<RenderPipeline>,
    opaque_pipeline: Arc<RenderPipeline>,
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
        args.lights
            .values()
            .enumerate()
            .map(|(idx, light)| -> CulledLightSet {
                let label = format_sso!("shadow cull {}", idx);
                profiling::scope!(&label);
                // TODO: This is hella duplicated
                let opaque_culled_objects = {
                    profiling::scope!("opaque");
                    match args.culler {
                        ModeData::CPU(cpu_culler) => cpu_culler.cull(CpuCullerCullArgs {
                            device: args.device,
                            camera: &light.camera,
                            interfaces: args.interfaces,
                            materials: args.materials,
                            objects: args.objects,
                            filter: |_, mat| mat.transparency == TransparencyType::Opaque,
                            sort: None,
                        }),
                        ModeData::GPU(gpu_culler) => {
                            args.encoder.push_debug_group(&label);
                            args.encoder.push_debug_group(&"opaque");
                            let culled = gpu_culler.cull(GpuCullerCullArgs {
                                device: args.device,
                                encoder: args.encoder,
                                interfaces: args.interfaces,
                                materials: args.materials,
                                camera: &light.camera,
                                objects: args.objects,
                                filter: |_, mat| mat.transparency == TransparencyType::Opaque,
                                sort: None,
                            });
                            args.encoder.pop_debug_group();
                            culled
                        }
                    }
                };

                let cutout_culled_objects = {
                    profiling::scope!("cutout");
                    match args.culler {
                        ModeData::CPU(cpu_culler) => cpu_culler.cull(CpuCullerCullArgs {
                            device: args.device,
                            camera: &light.camera,
                            interfaces: args.interfaces,
                            materials: args.materials,
                            objects: args.objects,
                            filter: |_, mat| mat.transparency == TransparencyType::Cutout,
                            sort: None,
                        }),
                        ModeData::GPU(gpu_culler) => {
                            args.encoder.push_debug_group(&"cutout");
                            let culled = gpu_culler.cull(GpuCullerCullArgs {
                                device: args.device,
                                encoder: args.encoder,
                                interfaces: args.interfaces,
                                materials: args.materials,
                                camera: &light.camera,
                                objects: args.objects,
                                filter: |_, mat| mat.transparency == TransparencyType::Cutout,
                                sort: None,
                            });
                            args.encoder.pop_debug_group();
                            args.encoder.pop_debug_group();
                            culled
                        }
                    }
                };

                let shadow_texture_arc = args.lights.get_layer_view_arc(light.shadow_tex);

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

            let mut rpass = args.encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some(&label),
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

            rpass.push_debug_group(TransparencyType::Opaque.to_debug_str());

            args.meshes.bind(&mut rpass);

            rpass.set_pipeline(&self.opaque_pipeline);
            rpass.set_bind_group(0, &args.samplers.linear_nearest_bg, &[]);
            rpass.set_bind_group(1, &light.opaque_culled_objects.output_bg, &[]);

            match light.opaque_culled_objects.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(&mut rpass, draws, args.samplers, 0, args.materials, 2),
                ModeData::GPU(ref data) => {
                    rpass.set_bind_group(2, args.materials.gpu_get_bind_group(), &[]);
                    rpass.set_bind_group(3, args.texture_bg.as_gpu(), &[]);
                    culling::gpu::run(&mut rpass, data);
                }
            }

            rpass.pop_debug_group();
            rpass.push_debug_group(TransparencyType::Cutout.to_debug_str());

            rpass.set_pipeline(&self.cutout_pipeline);
            rpass.set_bind_group(1, &light.opaque_culled_objects.output_bg, &[]);

            match light.cutout_culled_objects.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(&mut rpass, draws, args.samplers, 0, args.materials, 2),
                ModeData::GPU(ref data) => {
                    culling::gpu::run(&mut rpass, data);
                }
            }

            rpass.pop_debug_group();
        }
    }
}
