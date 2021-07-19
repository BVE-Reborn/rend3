use std::{mem, num::NonZeroU64};

use wgpu::{
    BindGroup, BindingResource, BindingType, BufferBindingType, CommandEncoder, CompareFunction, CullMode,
    DepthBiasState, DepthStencilState, Device, FragmentState, FrontFace, LoadOp, MultisampleState, Operations,
    PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPassColorAttachmentDescriptor,
    RenderPassDepthStencilAttachmentDescriptor, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, ShaderFlags,
    ShaderModuleDescriptor, ShaderStage, StencilState, TextureFormat, VertexState,
};

use crate::{ModeData, RendererMode, resources::{DirectionalLightManager, InternalObject, MaterialManager, TextureManager}, routines::{prepass::{BuildDepthPassShaderArgs, build_depth_pass_shader}, vertex::{cpu_vertex_buffers, gpu_vertex_buffers}}, shaders::SPIRV_SHADERS, util::bind_merge::BindGroupBuilder};

use super::{culling, CacheContext};

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
        let culling_results = match args.mode {
            RendererMode::CPUPowered => culling::cpu::cull(args.device, &light.camera, args.objects),
            RendererMode::GPUPowered => culling::gpu::cull(
                args.device,
                args.ctx,
                args.cull_encoder.as_gpu_mut(),
                args.materials,
                &light.camera,
                args.objects,
            ),
        };

        let depth_pass_data = build_depth_pass_shader(BuildDepthPassShaderArgs  {
            mode: args.mode,
            device: args.device,
            ctx: args.ctx, 
            culling_results: &culling_results,
        });

        let light_view = args.lights.get_layer_view_arc(idx as _);

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
                rpass.set_bind_group(3, args.texture_array_bg.as_gpu(), &[]);
                culling::gpu::run(&mut rpass, data);
            }
        }
    }
}
