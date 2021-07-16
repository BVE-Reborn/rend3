use std::{mem, num::NonZeroU64};

use wgpu::{
    BindGroup, BindingResource, BindingType, BufferBindingType, CommandEncoder, CompareFunction, CullMode,
    DepthBiasState, DepthStencilState, Device, FragmentState, FrontFace, LoadOp, MultisampleState, Operations,
    PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPassColorAttachmentDescriptor,
    RenderPassDepthStencilAttachmentDescriptor, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, ShaderFlags,
    ShaderModuleDescriptor, ShaderStage, StencilState, TextureFormat, VertexState,
};

use crate::{
    resources::{DirectionalLightManager, InternalObject, MaterialManager, TextureManager},
    routines::vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
    shaders::SPIRV_SHADERS,
    util::bind_merge::BindGroupBuilder,
    ModeData, RendererMode,
};

use super::{culling, CacheContext};

pub struct ShadowPassCullingArgs<'a, 'b> {
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

pub fn shadow_pass_culling<'a, 'b>(mut args: ShadowPassCullingArgs<'a, 'b>) {
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

        // TODO: encapsulate this somewhere
        let mut shader_object_bgb = BindGroupBuilder::new("shader objects");
        shader_object_bgb.append(
            ShaderStage::COMPUTE,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(mem::size_of::<culling::CullingOutput>() as _),
            },
            None,
            BindingResource::Buffer {
                buffer: &culling_results.output_buffer,
                offset: 0,
                size: None,
            },
        );
        let (shader_object_bgl, shader_object_bg) =
            shader_object_bgb.build_transient(&args.device, args.ctx.bind_group_cache);

        let depth_prepass_vert = args.ctx.sm_cache.shader_module(
            args.device,
            &ShaderModuleDescriptor {
                label: Some("depth pass vert"),
                source: wgpu::util::make_spirv(
                    SPIRV_SHADERS
                        .get_file(match args.mode {
                            RendererMode::CPUPowered => "depth.vert.cpu.spv",
                            RendererMode::GPUPowered => "depth.vert.gpu.spv",
                        })
                        .unwrap()
                        .contents(),
                ),
                flags: ShaderFlags::empty(),
            },
        );

        let depth_prepass_frag = args.ctx.sm_cache.shader_module(
            args.device,
            &ShaderModuleDescriptor {
                label: Some("depth pass frag"),
                source: wgpu::util::make_spirv(
                    SPIRV_SHADERS
                        .get_file(match args.mode {
                            RendererMode::CPUPowered => "depth.frag.cpu.spv",
                            RendererMode::GPUPowered => "depth.frag.gpu.spv",
                        })
                        .unwrap()
                        .contents(),
                ),
                flags: ShaderFlags::empty(),
            },
        );

        let cpu_vertex_buffers = cpu_vertex_buffers();
        let gpu_vertex_buffers = gpu_vertex_buffers();

        let pipeline = args.ctx.pipeline_cache.render_pipeline(
            args.device,
            &PipelineLayoutDescriptor {
                label: Some("depth prepass"),
                bind_group_layouts: &[&shader_object_bgl],
                push_constant_ranges: &[],
            },
            &RenderPipelineDescriptor {
                label: Some("depth prepass"),
                layout: None,
                vertex: VertexState {
                    module: &depth_prepass_vert,
                    entry_point: "main",
                    buffers: match args.mode {
                        RendererMode::CPUPowered => &cpu_vertex_buffers,
                        RendererMode::GPUPowered => &gpu_vertex_buffers,
                    },
                },
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: FrontFace::Cw,
                    cull_mode: CullMode::Back,
                    polygon_mode: PolygonMode::Fill,
                },
                depth_stencil: Some(DepthStencilState {
                    format: TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: CompareFunction::LessEqual,
                    stencil: StencilState::default(),
                    bias: DepthBiasState::default(),
                    clamp_depth: false,
                }),
                multisample: MultisampleState::default(),
                fragment: Some(FragmentState {
                    module: &depth_prepass_frag,
                    entry_point: "main",
                    targets: &[],
                }),
            },
        );

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
        rpass.set_pipeline(&pipeline);
        rpass.set_bind_group(0, args.linear_sampler_bg, &[]);
        rpass.set_bind_group(1, &shader_object_bg, &[]);

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
