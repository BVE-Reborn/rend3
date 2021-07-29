use std::{
    mem,
    num::{NonZeroU32, NonZeroU64},
};

use glam::{Mat3A, Mat4};
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BufferBindingType, Device,
    ShaderStage, TextureSampleType, TextureViewDimension,
};

use crate::{ModeData, resources::{CPUShaderMaterial, GPUShaderMaterial}, util::uniforms::ShaderCommonUniform};

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct PerObjectData {
    model_view: Mat4,
    model_view_proj: Mat4,
    inv_trans_model_view: Mat3A,
    // Unused in shader
    material_idx: u32,
}

unsafe impl bytemuck::Pod for PerObjectData {}
unsafe impl bytemuck::Zeroable for PerObjectData {}

pub struct ShaderInterfaces {
    pub samplers_bgl: BindGroupLayout,
    pub culled_object_bgl: BindGroupLayout,
    pub material_bgl: ModeData<BindGroupLayout, BindGroupLayout>,
    pub texture_bgl: ModeData<(), BindGroupLayout>,
    pub uniform_bgl: BindGroupLayout,
}

impl ShaderInterfaces {
    pub fn new(device: &Device, max_texture_count: ModeData<(), usize>) -> Self {
        let samplers_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("sampler bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStage::FRAGMENT,
                    ty: BindingType::Sampler {
                        filtering: true,
                        comparison: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStage::FRAGMENT,
                    ty: BindingType::Sampler {
                        filtering: false,
                        comparison: false,
                    },
                    count: None,
                },
            ],
        });

        let culled_object_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("culled object bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(mem::size_of::<PerObjectData>() as _),
                },
                count: None,
            }],
        });

        let material_bgl = max_texture_count.mode().into_data(
            || {
                let texture_binding = |idx| BindGroupLayoutEntry {
                    binding: idx,
                    visibility: ShaderStage::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                };

                device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: Some("cpu material bgl"),
                    entries: &[
                        texture_binding(0),
                        texture_binding(1),
                        texture_binding(2),
                        texture_binding(3),
                        texture_binding(4),
                        texture_binding(5),
                        texture_binding(6),
                        texture_binding(7),
                        texture_binding(8),
                        texture_binding(9),
                        BindGroupLayoutEntry {
                            binding: 10,
                            visibility: ShaderStage::FRAGMENT,
                            ty: BindingType::Buffer {
                                ty: BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: NonZeroU64::new(mem::size_of::<CPUShaderMaterial>() as _),
                            },
                            count: None,
                        },
                    ],
                })
            },
            || {
                device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                    label: Some("gpu material bgl"),
                    entries: &[BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStage::FRAGMENT,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: NonZeroU64::new(mem::size_of::<GPUShaderMaterial>() as _),
                        },
                        count: None,
                    }],
                })
            },
        );

        let texture_bgl = max_texture_count.map_gpu(|count| {
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("gpu texture array bgl"),
                entries: &[BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStage::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: NonZeroU32::new(count as _),
                }],
            })
        });

        let uniform_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("uniform bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(mem::size_of::<ShaderCommonUniform>() as _),
                },
                count: None,
            }],
        });

        Self {
            samplers_bgl,
            culled_object_bgl,
            material_bgl,
            texture_bgl,
            uniform_bgl,
        }
    }
}
