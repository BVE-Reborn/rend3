use arrayvec::ArrayVec;
use rend3::{
    types::{Handedness, SampleCount},
    Renderer, RendererDataCore, RendererMode,
};
use wgpu::{
    BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState, Face,
    FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, StencilState, TextureFormat, VertexState,
};

use crate::{
    common::{interfaces::ShaderInterfaces, shaders::mode_safe_shader},
    material::PbrMaterial,
    vertex::{cpu_vertex_buffers, gpu_vertex_buffers},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepthPassType {
    Shadow,
    Prepass,
}

#[derive(Clone)]
pub struct BuildDepthPassShaderArgs<'a> {
    pub renderer: &'a Renderer,
    pub data_core: &'a RendererDataCore,

    pub interfaces: &'a ShaderInterfaces,

    pub samples: SampleCount,
    pub ty: DepthPassType,
    pub unclipped_depth_supported: bool,
}

pub struct DepthPassPipelines {
    pub cutout: RenderPipeline,
    pub opaque: RenderPipeline,
}

pub fn build_depth_pass_pipeline(args: BuildDepthPassShaderArgs) -> DepthPassPipelines {
    profiling::scope!("build depth pass pipelines");
    let depth_vert = unsafe {
        mode_safe_shader(
            &args.renderer.device,
            args.renderer.mode,
            "depth pass vert",
            "depth.vert.cpu.wgsl",
            "depth.vert.gpu.spv",
        )
    };

    let depth_opaque_frag = unsafe {
        mode_safe_shader(
            &args.renderer.device,
            args.renderer.mode,
            "depth pass opaque frag",
            "depth-opaque.frag.cpu.wgsl",
            "depth-opaque.frag.gpu.spv",
        )
    };

    let depth_cutout_frag = unsafe {
        mode_safe_shader(
            &args.renderer.device,
            args.renderer.mode,
            "depth pass cutout frag",
            "depth-cutout.frag.cpu.wgsl",
            "depth-cutout.frag.gpu.spv",
        )
    };

    let mut bgls: ArrayVec<&BindGroupLayout, 4> = ArrayVec::new();
    bgls.push(match args.ty {
        DepthPassType::Shadow => &args.interfaces.shadow_uniform_bgl,
        DepthPassType::Prepass => &args.interfaces.forward_uniform_bgl,
    });
    bgls.push(&args.interfaces.per_material_bgl);
    if args.renderer.mode == RendererMode::GPUPowered {
        bgls.push(args.data_core.d2_texture_manager.gpu_bgl())
    } else {
        bgls.push(
            args.data_core
                .material_manager
                .get_bind_group_layout_cpu::<PbrMaterial>(),
        );
    }

    let pll = args.renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("depth prepass"),
        bind_group_layouts: &bgls,
        push_constant_ranges: &[],
    });

    DepthPassPipelines {
        opaque: create_depth_inner(
            &args,
            &pll,
            &depth_vert,
            &depth_opaque_frag,
            match args.ty {
                DepthPassType::Prepass => "depth opaque prepass",
                DepthPassType::Shadow => "shadow opaque prepass",
            },
        ),
        cutout: create_depth_inner(
            &args,
            &pll,
            &depth_vert,
            &depth_cutout_frag,
            match args.ty {
                DepthPassType::Prepass => "depth cutout prepass",
                DepthPassType::Shadow => "shadow cutout prepass",
            },
        ),
    }
}

fn create_depth_inner(
    args: &BuildDepthPassShaderArgs,
    pll: &wgpu::PipelineLayout,
    vert: &wgpu::ShaderModule,
    frag: &wgpu::ShaderModule,
    name: &str,
) -> RenderPipeline {
    profiling::scope!("build depth pipeline", name);
    let color_state = [ColorTargetState {
        format: TextureFormat::Rgba16Float,
        blend: None,
        write_mask: ColorWrites::empty(),
    }];
    let cpu_vertex_buffers = cpu_vertex_buffers();
    let gpu_vertex_buffers = gpu_vertex_buffers();
    args.renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(name),
        layout: Some(pll),
        vertex: VertexState {
            module: vert,
            entry_point: "main",
            buffers: match args.renderer.mode {
                RendererMode::CPUPowered => &cpu_vertex_buffers,
                RendererMode::GPUPowered => &gpu_vertex_buffers,
            },
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: match args.renderer.handedness {
                Handedness::Left => FrontFace::Cw,
                Handedness::Right => FrontFace::Ccw,
            },
            cull_mode: Some(match args.ty {
                DepthPassType::Shadow => Face::Front,
                DepthPassType::Prepass => Face::Back,
            }),
            unclipped_depth: matches!(args.ty, DepthPassType::Shadow) && args.unclipped_depth_supported,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState::default(),
            bias: match args.ty {
                DepthPassType::Prepass => DepthBiasState::default(),
                DepthPassType::Shadow => DepthBiasState {
                    constant: -2,
                    slope_scale: -2.0,
                    clamp: 0.0,
                },
            },
        }),
        multisample: MultisampleState {
            count: args.samples as u32,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: frag,
            entry_point: "main",
            targets: match args.ty {
                DepthPassType::Prepass => &color_state,
                DepthPassType::Shadow => &[],
            },
        }),
        multiview: None,
    })
}
