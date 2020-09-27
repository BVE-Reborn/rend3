use crate::{
    renderer::{INTERNAL_RENDERBUFFER_DEPTH_FORMAT, INTERNAL_RENDERBUFFER_FORMAT, SWAPCHAIN_FORMAT},
    VSyncMode,
};
use std::num::NonZeroU8;
use wgpu::{
    AddressMode, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Device, Extent3d, FilterMode, PresentMode, Sampler,
    SamplerDescriptor, ShaderStage, Surface, SwapChain, SwapChainDescriptor, Texture, TextureComponentType,
    TextureDescriptor, TextureDimension, TextureUsage, TextureView, TextureViewDescriptor, TextureViewDimension,
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
                VSyncMode::Off => PresentMode::Mailbox,
            },
        },
    )
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

pub fn create_object_input_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object input bgl"),
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
                visibility: ShaderStage::VERTEX,
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
    let entry = BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStage::VERTEX | ShaderStage::COMPUTE,
        ty: BindingType::StorageBuffer {
            dynamic: false,
            min_binding_size: None,
            readonly: false,
        },
        count: None,
    };

    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object output bgl"),
        entries: &[
            entry.clone(),
            BindGroupLayoutEntry {
                binding: 1,
                ..entry.clone()
            },
            BindGroupLayoutEntry { binding: 2, ..entry },
        ],
    })
}
pub fn create_object_output_noindirect_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object output bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
            ty: BindingType::StorageBuffer {
                dynamic: false,
                min_binding_size: None,
                readonly: false,
            },
            count: None,
        }],
    })
}

pub fn create_material_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("material bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::FRAGMENT,
            ty: BindingType::UniformBuffer {
                dynamic: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

pub fn create_uniform_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("uniform bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::COMPUTE,
            ty: BindingType::UniformBuffer {
                dynamic: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

pub fn create_sampler(device: &Device) -> Sampler {
    device.create_sampler(&SamplerDescriptor {
        label: Some("linear sampler"),
        address_mode_u: AddressMode::Repeat,
        address_mode_v: AddressMode::Repeat,
        address_mode_w: AddressMode::Repeat,
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        lod_min_clamp: -100.0,
        lod_max_clamp: 100.0,
        compare: None,
        anisotropy_clamp: NonZeroU8::new(16),
    })
}

#[derive(Debug, Copy, Clone)]
pub enum FramebufferTextureKind {
    Color,
    Depth,
}

pub fn create_framebuffer_texture(
    device: &Device,
    size: PhysicalSize<u32>,
    kind: FramebufferTextureKind,
) -> (Texture, TextureView) {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("RenderBuffer Depth Texture"),
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
            FramebufferTextureKind::Depth => INTERNAL_RENDERBUFFER_DEPTH_FORMAT,
        },
        usage: TextureUsage::SAMPLED | TextureUsage::OUTPUT_ATTACHMENT,
    });

    let view = texture.create_view(&TextureViewDescriptor::default());

    (texture, view)
}

macro_rules! create_vertex_buffer_descriptor {
    () => {
        wgpu::VertexBufferDescriptor {
            stride: std::mem::size_of::<crate::datatypes::ModelVertex>() as u64,
            step_mode: wgpu::InputStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float3, 1 => Float3, 2 => Float2, 3 => Uchar4Norm, 4 => Uint],
        }
    };
}
