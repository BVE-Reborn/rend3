use std::sync::Arc;

use wgpu::{
    BindGroup, CommandEncoder, Device, LoadOp, Operations, RenderPassDepthStencilAttachment, RenderPassDescriptor,
    RenderPipeline, TextureView,
};

use crate::{
    resources::{DirectionalLightManager, InternalObject, MaterialManager, MeshBuffers},
    routines::{
        common::{interfaces::ShaderInterfaces, samplers::Samplers},
        culling::{
            cpu::{CpuCuller, CpuCullerCullArgs},
            gpu::{GpuCuller, GpuCullerCullArgs},
            CulledObjectSet,
        },
    },
    ModeData,
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
    pub culled_objects: CulledObjectSet,
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
    pipeline: Arc<RenderPipeline>,
}

impl DirectionalShadowPass {
    pub fn new(pipeline: Arc<RenderPipeline>) -> Self {
        Self { pipeline }
    }

    pub fn cull_shadows(&self, args: DirectionalShadowPassCullShadowsArgs<'_>) -> Vec<CulledLightSet> {
        args.lights
            .values()
            .map(|light| {
                let culled_objects = match args.culler {
                    ModeData::CPU(cpu_culler) => cpu_culler.cull(CpuCullerCullArgs {
                        device: args.device,
                        camera: &light.camera,
                        interfaces: args.interfaces,
                        objects: args.objects,
                    }),
                    ModeData::GPU(gpu_culler) => gpu_culler.cull(GpuCullerCullArgs {
                        device: args.device,
                        encoder: args.encoder,
                        interfaces: args.interfaces,
                        materials: args.materials,
                        camera: &light.camera,
                        objects: args.objects,
                    }),
                };

                let shadow_texture_arc = args.lights.get_layer_view_arc(light.shadow_tex);

                CulledLightSet {
                    culled_objects,
                    shadow_texture_arc,
                }
            })
            .collect()
    }

    pub fn draw_culled_shadows(&self, args: DirectionalShadowPassDrawCulledShadowsArgs<'_>) {
        for light in args.culled_lights {
            let mut rpass = args.encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("culling pass"),
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

            args.meshes.bind(&mut rpass);

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, &args.samplers.linear_nearest_bg, &[]);
            rpass.set_bind_group(1, &light.culled_objects.output_bg, &[]);

            match light.culled_objects.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(&mut rpass, &draws, args.samplers, 0, args.materials, 2),
                ModeData::GPU(ref data) => {
                    rpass.set_bind_group(2, args.materials.gpu_get_bind_group(), &[]);
                    rpass.set_bind_group(3, args.texture_bg.as_gpu(), &[]);
                    culling::gpu::run(&mut rpass, data);
                }
            }
        }
    }
}
