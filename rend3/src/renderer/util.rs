use crate::{
    renderer::{RendererMode, SWAPCHAIN_FORMAT},
    VSyncMode,
};
use std::num::NonZeroU8;
use wgpu::{
    AddressMode, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, CompareFunction,
    Device, FilterMode, PresentMode, Sampler, SamplerDescriptor, ShaderStage, Surface, SwapChain, SwapChainDescriptor,
    TextureComponentType, TextureUsage, TextureViewDimension,
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

pub fn create_object_input_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object input bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::COMPUTE,
            ty: BindingType::StorageBuffer {
                dynamic: false,
                min_binding_size: None,
                readonly: true,
            },
            count: None,
        }],
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

pub fn create_general_bind_group_layout(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("general bind group"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::Sampler { comparison: false },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::Sampler { comparison: true },
                count: None,
            },
        ],
    })
}

pub fn create_object_data_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("object data bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
            ty: BindingType::StorageBuffer {
                dynamic: false,
                min_binding_size: None,
                readonly: true,
            },
            count: None,
        }],
    })
}

pub fn create_material_bgl(device: &Device, mode: RendererMode) -> BindGroupLayout {
    match mode {
        RendererMode::CPUPowered => {
            let texture_entry = BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::SampledTexture {
                    dimension: TextureViewDimension::D2,
                    component_type: TextureComponentType::Float,
                    multisampled: false,
                },
                count: None,
            };
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("material data bgl"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        ..texture_entry.clone()
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        ..texture_entry.clone()
                    },
                    BindGroupLayoutEntry {
                        binding: 2,
                        ..texture_entry.clone()
                    },
                    BindGroupLayoutEntry {
                        binding: 3,
                        ..texture_entry.clone()
                    },
                    BindGroupLayoutEntry {
                        binding: 4,
                        ..texture_entry.clone()
                    },
                    BindGroupLayoutEntry {
                        binding: 5,
                        ..texture_entry.clone()
                    },
                    BindGroupLayoutEntry {
                        binding: 6,
                        ..texture_entry.clone()
                    },
                    BindGroupLayoutEntry {
                        binding: 7,
                        ..texture_entry.clone()
                    },
                    BindGroupLayoutEntry {
                        binding: 8,
                        ..texture_entry
                    },
                    BindGroupLayoutEntry {
                        binding: 9,
                        visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                        ty: BindingType::UniformBuffer {
                            dynamic: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            })
        }
        RendererMode::GPUPowered => device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("material data bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::StorageBuffer {
                    readonly: true,
                    dynamic: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        }),
    }
}

pub fn create_camera_data_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("camera data bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
            ty: BindingType::UniformBuffer {
                dynamic: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}

pub fn create_shadow_texture_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("shadow texture bgl"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::StorageBuffer {
                    dynamic: false,
                    min_binding_size: None,
                    readonly: true,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::SampledTexture {
                    dimension: TextureViewDimension::D2Array,
                    component_type: TextureComponentType::Float,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

pub fn create_skybox_bgl(device: &Device) -> BindGroupLayout {
    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("skybox bgl"),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
            ty: BindingType::SampledTexture {
                dimension: TextureViewDimension::Cube,
                component_type: TextureComponentType::Float,
                multisampled: false,
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
    })
}
