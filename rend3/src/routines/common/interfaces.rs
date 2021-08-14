use std::{mem, num::NonZeroU64};

use glam::{Mat3A, Mat4};
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BufferBindingType, Device,
    ShaderStages, TextureSampleType, TextureViewDimension,
};

use crate::routines::uniforms::ShaderCommonUniform;

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct PerObjectData {
    pub model_view: Mat4,
    pub model_view_proj: Mat4,
    pub inv_trans_model_view: Mat3A,
    // Unused in shader
    pub material_idx: u32,
}

unsafe impl bytemuck::Pod for PerObjectData {}
unsafe impl bytemuck::Zeroable for PerObjectData {}

pub struct ShaderInterfaces {
    // TODO: move this into samplers struct?
    pub samplers_bgl: BindGroupLayout,
    pub culled_object_bgl: BindGroupLayout,
    pub uniform_bgl: BindGroupLayout,
    pub blit_bgl: BindGroupLayout,
}

impl ShaderInterfaces {
    pub fn new(device: &Device) -> Self {
        let samplers_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("samplers bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        filtering: true,
                        comparison: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        filtering: true,
                        comparison: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler {
                        filtering: true,
                        comparison: true,
                    },
                    count: None,
                },
            ],
        });

        let culled_object_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("culled object bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(mem::size_of::<PerObjectData>() as _),
                },
                count: None,
            }],
        });

        let uniform_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("uniform bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX | ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: NonZeroU64::new(mem::size_of::<ShaderCommonUniform>() as _),
                },
                count: None,
            }],
        });

        let blit_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("blit bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });

        Self {
            samplers_bgl,
            culled_object_bgl,
            uniform_bgl,
            blit_bgl,
        }
    }
}
