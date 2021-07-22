use std::{mem, num::NonZeroU64, sync::Arc};

use wgpu::{
    BindGroup, BindingResource, BindingType, BufferBindingType, CompareFunction, CullMode, DepthBiasState,
    DepthStencilState, Device, FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, RenderPass, RenderPipeline, RenderPipelineDescriptor, ShaderFlags,
    ShaderModuleDescriptor, ShaderStage, StencilState, TextureFormat, VertexState,
};

use crate::{
    resources::{CameraManager, MaterialManager},
    routines::{
        culling::{self, CulledObjectSet, PerObjectData},
        vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
        CacheContext,
    },
    shaders::SPIRV_SHADERS,
    util::bind_merge::BindGroupBuilder,
    ModeData, RendererMode,
};

pub struct BuildDepthPassShaderArgs<'a, 'b> {
    pub mode: RendererMode,
    pub device: &'a Device,
    pub ctx: &'a mut CacheContext<'b>,
    pub culling_results: &'a CulledObjectSet,
}

pub struct BuildDepthPassShaderResult {
    pub pipeline: Arc<RenderPipeline>,
    pub shader_objects_bg: Arc<BindGroup>,
}

pub fn build_depth_pass_shader(mut args: BuildDepthPassShaderArgs) -> BuildDepthPassShaderResult {
    // TODO: encapsulate this somewhere
    let mut shader_objects_bgb = BindGroupBuilder::new("shader objects");
    shader_objects_bgb.append(
        ShaderStage::COMPUTE,
        BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: NonZeroU64::new(mem::size_of::<culling::PerObjectData>() as _),
        },
        None,
        BindingResource::Buffer {
            buffer: &args.culling_results.output_buffer,
            offset: 0,
            size: None,
        },
    );
    let (shader_objects_bgl, shader_objects_bg) =
        shader_objects_bgb.build_transient(&args.device, args.ctx.bind_group_cache);

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
            bind_group_layouts: &[&shader_objects_bgl],
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

    BuildDepthPassShaderResult {
        pipeline,
        shader_objects_bg,
    }
}

pub struct DepthPrepassArgs<'a, 'b> {
    mode: RendererMode,
    device: &'a Device,
    ctx: &'a mut CacheContext<'b>,
    rpass: &'a mut RenderPass<'b>,
    materials: &'a MaterialManager,
    camera: &'a CameraManager,
    texture_array_bg: ModeData<(), &'a BindGroup>,
    linear_sampler_bg: &'a BindGroup,
    culling_results: &'a CulledObjectSet,
}

pub fn depth_prepass<'a, 'b>(mut args: DepthPrepassArgs<'a, 'b>) {
    let depth_pass_data = build_depth_pass_shader(BuildDepthPassShaderArgs {
        mode: args.mode,
        device: args.device,
        ctx: args.ctx,
        culling_results: args.culling_results,
    });

    args.rpass.set_pipeline(&depth_pass_data.pipeline);
    args.rpass.set_bind_group(0, args.linear_sampler_bg, &[]);
    args.rpass.set_bind_group(1, &depth_pass_data.shader_objects_bg, &[]);

    match args.culling_results.calls {
        ModeData::CPU(ref draws) => culling::cpu::run(&mut rpass, &draws, args.materials, 2),
        ModeData::GPU(ref data) => {
            args.rpass.set_bind_group(2, &args.material_gpu_bg.as_gpu().1, &[]);
            rpass.set_bind_group(3, args.texture_array_bg.as_gpu(), &[]);
            culling::gpu::run(&mut rpass, data);
        }
    }
}
