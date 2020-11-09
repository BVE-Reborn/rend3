use crate::{
    renderer::{
        INTERNAL_RENDERBUFFER_DEPTH_FORMAT, INTERNAL_RENDERBUFFER_FORMAT, INTERNAL_RENDERBUFFER_NORMAL_FORMAT,
        INTERNAL_SHADOW_DEPTH_FORMAT, SWAPCHAIN_FORMAT,
    },
    VSyncMode,
};
use arrayvec::ArrayVec;
use std::num::NonZeroU8;
use wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendDescriptor, ColorStateDescriptor, ColorWrite,
    CompareFunction, CullMode, DepthStencilStateDescriptor, Device, Extent3d, FilterMode, FrontFace, IndexFormat,
    PipelineLayout, PolygonMode, PresentMode, PrimitiveTopology, ProgrammableStageDescriptor,
    RasterizationStateDescriptor, RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerDescriptor, ShaderModule,
    ShaderStage, StencilStateDescriptor, Surface, SwapChain, SwapChainDescriptor, Texture, TextureComponentType,
    TextureDescriptor, TextureDimension, TextureUsage, TextureView, TextureViewDescriptor, TextureViewDimension,
    VertexStateDescriptor,
};
use winit::dpi::PhysicalSize;

pub fn create_swapchain(device: &Device, surface: &Surface, size: PhysicalSize<u32>, vsync: VSyncMode) -> SwapChain {
    device.create_swap_chain(
        &surface,
        &SwapChainDescriptor {
            width: size.width,
            height: size.height,
            usage: TextureUsage::OUTPUT_ATTACHMENT,
            format: SWAPCHAIN_FORMAT,
            present_mode: match vsync {
                VSyncMode::On => PresentMode::Fifo,
                VSyncMode::Off => PresentMode::Immediate,
            },
        },
    )
}

pub fn create_prefix_sum_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("prefix sum bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::COMPUTE,
                ty: BindingType::StorageBuffer {
                    dynamic: false,
                    min_binding_size: None,
                    readonly: true,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStage::COMPUTE,
                ty: BindingType::StorageBuffer {
                    dynamic: false,
                    min_binding_size: None,
                    readonly: false,
                },
                count: None,
            },
        ],
    })
}

pub fn create_blit_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("blit bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::SampledTexture {
                    dimension: TextureViewDimension::D2,
                    component_type: TextureComponentType::Float,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::Sampler { comparison: false },
                count: None,
            },
        ],
    })
}

pub fn create_blit_bg(
    device: &Device,
    blit_bgl: &BindGroupLayout,
    source_image: &TextureView,
    sampler: &Sampler,
) -> BindGroup {
    device.create_bind_group(&BindGroupDescriptor {
        label: Some("blit bgl"),
        layout: blit_bgl,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(source_image),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub fn create_pre_cull_bgl(device: &Device) -> BindGroupLayout {
    let entry = BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStage::COMPUTE,
        ty: BindingType::StorageBuffer {
            dynamic: false,
            min_binding_size: None,
            readonly: false,
        },
        count: None,
    };

    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("pre-cull bgl"),
        entries: &[entry.clone(), BindGroupLayoutEntry { binding: 1, ..entry }],
    })
}

pub fn create_general_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("general bind group"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::COMPUTE,
                ty: BindingType::StorageBuffer {
                    dynamic: false,
                    min_binding_size: None,
                    readonly: true,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::UniformBuffer {
                    dynamic: false,
                    min_binding_size: None,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::Sampler { comparison: false },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::Sampler { comparison: true },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 4,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::StorageBuffer {
                    dynamic: false,
                    min_binding_size: None,
                    readonly: true,
                },
                count: None,
            },
        ],
    })
}

pub fn create_object_output_bgl(device: &Device) -> BindGroupLayout {
    let entry = |binding: u32, readonly: bool| BindGroupLayoutEntry {
        binding,
        visibility: ShaderStage::COMPUTE,
        ty: BindingType::StorageBuffer {
            dynamic: false,
            min_binding_size: None,
            readonly,
        },
        count: None,
    };

    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object output bgl"),
        entries: &[
            entry(0, true),
            entry(1, true),
            entry(2, false),
            entry(3, false),
            entry(4, false),
        ],
    })
}
pub fn create_object_output_noindirect_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object output noindirect bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::StorageBuffer {
                    dynamic: false,
                    min_binding_size: None,
                    readonly: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStage::COMPUTE | ShaderStage::FRAGMENT,
                ty: BindingType::UniformBuffer {
                    dynamic: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

pub fn create_uniform_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("uniform bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::COMPUTE | ShaderStage::FRAGMENT,
            ty: BindingType::UniformBuffer {
                dynamic: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

#[allow(dead_code)]
pub enum SamplerType {
    Nearest,
    Linear,
    Shadow,
}

pub fn create_sampler(device: &Device, ty: SamplerType) -> Sampler {
    let filter = match ty {
        SamplerType::Nearest => FilterMode::Nearest,
        SamplerType::Linear => FilterMode::Linear,
        SamplerType::Shadow => FilterMode::Linear,
    };

    let compare = match ty {
        SamplerType::Nearest | SamplerType::Linear => None,
        SamplerType::Shadow => Some(CompareFunction::LessEqual),
    };

    device.create_sampler(&SamplerDescriptor {
        label: Some(match ty {
            SamplerType::Linear => "linear sampler",
            SamplerType::Shadow => "shadow sampler",
            SamplerType::Nearest => "nearest sampler",
        }),
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::Repeat,
        address_mode_w: AddressMode::Repeat,
        mag_filter: filter,
        min_filter: filter,
        mipmap_filter: filter,
        lod_min_clamp: -100.0,
        lod_max_clamp: 100.0,
        compare,
        anisotropy_clamp: match ty {
            SamplerType::Linear => NonZeroU8::new(16),
            SamplerType::Shadow | SamplerType::Nearest => None,
        },
        border_color: None,
    })
}

#[derive(Debug, Copy, Clone)]
pub enum FramebufferTextureKind {
    Color,
    Depth,
    Normal,
}

pub fn create_framebuffer_texture(
    device: &Device,
    size: PhysicalSize<u32>,
    kind: FramebufferTextureKind,
) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some(match kind {
            FramebufferTextureKind::Color => "RenderBuffer Color Texture",
            FramebufferTextureKind::Depth => "RenderBuffer Depth Texture",
            FramebufferTextureKind::Normal => "RenderBuffer Normal Texture",
        }),
        size: Extent3d {
            width: size.width,
            height: size.height,
            depth: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: match kind {
            FramebufferTextureKind::Color => INTERNAL_RENDERBUFFER_FORMAT,
            FramebufferTextureKind::Normal => INTERNAL_RENDERBUFFER_NORMAL_FORMAT,
            FramebufferTextureKind::Depth => INTERNAL_RENDERBUFFER_DEPTH_FORMAT,
        },
        usage: TextureUsage::SAMPLED | TextureUsage::OUTPUT_ATTACHMENT,
    });

    let view = texture.create_view(&TextureViewDescriptor::default());

    (texture, view)
}

macro_rules! create_vertex_buffer_descriptors {
    () => {
        [
            wgpu::VertexBufferDescriptor {
                stride: std::mem::size_of::<crate::datatypes::ModelVertex>() as u64,
                step_mode: wgpu::InputStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float3, 1 => Float3, 2 => Float2, 3 => Uchar4Norm, 4 => Uint],
            },
            wgpu::VertexBufferDescriptor {
                stride: 20,
                step_mode: wgpu::InputStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttributeDescriptor {
                        format: wgpu::VertexFormat::Uint,
                        offset: 16,
                        shader_location: 5
                    }
                ],
            }
        ]
    };
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum RenderPipelineType {
    Depth,
    Shadow,
    Opaque,
    Skybox,
}

pub fn create_render_pipeline(
    device: &Device,
    layout: &PipelineLayout,
    vertex: &ShaderModule,
    fragment: &ShaderModule,
    ty: RenderPipelineType,
) -> RenderPipeline {
    let vertex_buffers = create_vertex_buffer_descriptors!();

    let mut color_states: ArrayVec<[ColorStateDescriptor; 2]> = ArrayVec::new();
    if ty != RenderPipelineType::Shadow {
        color_states.push(ColorStateDescriptor {
            format: INTERNAL_RENDERBUFFER_FORMAT,
            alpha_blend: BlendDescriptor::REPLACE,
            color_blend: BlendDescriptor::REPLACE,
            write_mask: match ty {
                RenderPipelineType::Depth => ColorWrite::empty(),
                RenderPipelineType::Opaque | RenderPipelineType::Skybox => ColorWrite::ALL,
                RenderPipelineType::Shadow => unreachable!(),
            },
        });
        color_states.push(ColorStateDescriptor {
            format: INTERNAL_RENDERBUFFER_NORMAL_FORMAT,
            alpha_blend: BlendDescriptor::REPLACE,
            color_blend: BlendDescriptor::REPLACE,
            write_mask: match ty {
                RenderPipelineType::Depth | RenderPipelineType::Skybox => ColorWrite::empty(),
                RenderPipelineType::Opaque => ColorWrite::ALL,
                RenderPipelineType::Shadow => unreachable!(),
            },
        });
    }

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(match ty {
            RenderPipelineType::Depth => "depth pipeline",
            RenderPipelineType::Shadow => "shadow pipeline",
            RenderPipelineType::Opaque => "opaque pipeline",
            RenderPipelineType::Skybox => "skybox pipeline",
        }),
        layout: Some(&layout),
        vertex_stage: ProgrammableStageDescriptor {
            module: &vertex,
            entry_point: "main",
        },
        fragment_stage: Some(ProgrammableStageDescriptor {
            module: &fragment,
            entry_point: "main",
        }),
        rasterization_state: Some(RasterizationStateDescriptor {
            front_face: FrontFace::Cw,
            cull_mode: match ty {
                RenderPipelineType::Shadow | RenderPipelineType::Opaque | RenderPipelineType::Depth => CullMode::Back,
                RenderPipelineType::Skybox => CullMode::None,
            },
            polygon_mode: PolygonMode::Fill,
            clamp_depth: match ty {
                RenderPipelineType::Shadow => false,
                // RenderPipelineType::Shadow => true,
                RenderPipelineType::Skybox | RenderPipelineType::Opaque | RenderPipelineType::Depth => false,
            },
            depth_bias: match ty {
                RenderPipelineType::Shadow => 2,
                RenderPipelineType::Skybox | RenderPipelineType::Opaque | RenderPipelineType::Depth => 0,
            },
            depth_bias_slope_scale: match ty {
                RenderPipelineType::Shadow => 2.0,
                RenderPipelineType::Skybox | RenderPipelineType::Opaque | RenderPipelineType::Depth => 0.0,
            },
            depth_bias_clamp: 0.0,
        }),
        primitive_topology: PrimitiveTopology::TriangleList,
        color_states: &color_states,
        depth_stencil_state: Some(DepthStencilStateDescriptor {
            format: match ty {
                RenderPipelineType::Shadow => INTERNAL_SHADOW_DEPTH_FORMAT,
                RenderPipelineType::Depth | RenderPipelineType::Opaque | RenderPipelineType::Skybox => {
                    INTERNAL_RENDERBUFFER_DEPTH_FORMAT
                }
            },
            depth_write_enabled: match ty {
                RenderPipelineType::Shadow | RenderPipelineType::Depth => true,
                RenderPipelineType::Opaque | RenderPipelineType::Skybox => false,
            },
            depth_compare: match ty {
                RenderPipelineType::Shadow => CompareFunction::LessEqual,
                RenderPipelineType::Depth => CompareFunction::Greater,
                RenderPipelineType::Opaque | RenderPipelineType::Skybox => CompareFunction::Equal,
            },
            stencil: StencilStateDescriptor::default(),
        }),
        vertex_state: VertexStateDescriptor {
            index_format: IndexFormat::Uint32,
            vertex_buffers: match ty {
                RenderPipelineType::Shadow | RenderPipelineType::Depth | RenderPipelineType::Opaque => &vertex_buffers,
                RenderPipelineType::Skybox => &[],
            },
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
    })
}
