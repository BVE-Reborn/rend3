use std::{mem, num::NonZeroU64, sync::Arc};

use wgpu::{
    BindGroup, BindingResource, BindingType, BufferBindingType, CommandEncoder, CompareFunction, CullMode,
    DepthBiasState, DepthStencilState, Device, FragmentState, FrontFace, LoadOp, MultisampleState, Operations,
    PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPassColorAttachmentDescriptor,
    RenderPassDepthStencilAttachmentDescriptor, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    Sampler, ShaderFlags, ShaderModuleDescriptor, ShaderStage, StencilState, TextureFormat, TextureView, VertexState,
};

use crate::{
    resources::{DirectionalLightManager, InternalObject, MaterialManager, TextureManager},
    routines::{
        common::interfaces::ShaderInterfaces,
        culling::{
            cpu::{CpuCuller, CpuCullerCullArgs},
            gpu::{GpuCuller, GpuCullerCullArgs},
            CulledObjectSet,
        },
        prepass::{build_depth_pass_shader, BuildDepthPassShaderArgs},
        vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
    },
    shaders::SPIRV_SHADERS,
    util::bind_merge::BindGroupBuilder,
    ModeData, RendererMode,
};

use super::{culling, CacheContext};

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
    pub textures: &'a TextureManager,

    pub sampler_bg: &'a BindGroup,

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
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachmentDescriptor {
                    attachment: &light.shadow_texture_arc,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(0.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            rpass.set_pipeline(&self.pipeline);
            rpass.set_bind_group(0, args.sampler_bg, &[]);
            rpass.set_bind_group(1, &light.culled_objects.output_bg, &[]);

            match light.culled_objects.calls {
                ModeData::CPU(ref draws) => culling::cpu::run(&mut rpass, &draws, args.materials, 2),
                ModeData::GPU(ref data) => {
                    // TODO(ref): Figure out how to get materials or textures.
                    rpass.set_bind_group(2, &args.materials.gpu_make_bg(device, cache, visibility).as_gpu(), &[]);
                    rpass.set_bind_group(3, args.textures.gpu, &[]);
                    culling::gpu::run(&mut rpass, data);
                }
            }
        }
    }
}

pub struct DirectionalShadowPassArgs<'a, 'b> {
    mode: RendererMode,
    device: &'a Device,
    ctx: &'a mut CacheContext<'b>,
    cull_encoder: ModeData<(), &'a mut CommandEncoder>,
    render_encoder: &'a mut CommandEncoder,
    materials: &'a MaterialManager,
    lights: &'a DirectionalLightManager,
    texture_array_bg: ModeData<(), &'a BindGroup>,
    linear_sampler_bg: &'a BindGroup,
    objects: &'a [InternalObject],
}

pub fn directional_shadow_pass<'a, 'b>(mut args: DirectionalShadowPassArgs<'a, 'b>) {
    let material_gpu_bg = args.mode.into_data(
        || (),
        || {
            args.materials
                .gpu_make_bg(args.device, args.ctx.bind_group_cache, ShaderStage::FRAGMENT)
        },
    );

    for (idx, light) in args.lights.values().enumerate() {
        let mut rpass = args.render_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("culling pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachmentDescriptor {
                attachment: &light_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(0.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });
        rpass.set_pipeline(&depth_pass_data.pipeline);
        rpass.set_bind_group(0, args.linear_sampler_bg, &[]);
        rpass.set_bind_group(1, &depth_pass_data.shader_objects_bg, &[]);

        match culling_results.calls {
            ModeData::CPU(ref draws) => culling::cpu::run(&mut rpass, &draws, args.materials, 2),
            ModeData::GPU(ref data) => {
                rpass.set_bind_group(2, &material_gpu_bg.as_gpu().1, &[]);
                rpass.set_bind_group(3, args.te.as_gpu(), &[]);
                culling::gpu::run(&mut rpass, data);
            }
        }
    }
}
