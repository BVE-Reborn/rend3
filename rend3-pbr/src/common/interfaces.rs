use std::{mem, num::NonZeroU64};

use glam::{Mat4, Vec3A};
use rend3::{
    managers::{DirectionalLightManager, MaterialManager},
    util::bind_merge::BindGroupLayoutBuilder,
    RendererMode,
};
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BufferBindingType, Device,
    ShaderStages, TextureSampleType, TextureViewDimension,
};

use crate::{common::samplers::Samplers, material::PbrMaterial, uniforms::ShaderCommonUniform};

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct PerObjectData {
    pub model_view: Mat4,
    pub model_view_proj: Mat4,
    pub inv_squared_scale: Vec3A,
    // Unused in shader
    pub material_idx: u32,
}

unsafe impl bytemuck::Pod for PerObjectData {}
unsafe impl bytemuck::Zeroable for PerObjectData {}

pub struct ShaderInterfaces {
    pub bulk_bgl: BindGroupLayout,

    pub blit_bgl: BindGroupLayout,
    pub skybox_bgl: BindGroupLayout,
}

impl ShaderInterfaces {
    pub fn new(device: &Device, mode: RendererMode) -> Self {
        profiling::scope!("ShaderInterfaces::new");

        let mut bglb = BindGroupLayoutBuilder::new();

        Samplers::add_to_bgl(&mut bglb);

        bglb.append(
            ShaderStages::VERTEX,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(mem::size_of::<PerObjectData>() as _),
            },
            None,
        );
        
        DirectionalLightManager::add_to_bgl(&mut bglb);

        bglb.append(
            ShaderStages::VERTEX_FRAGMENT,
            BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(mem::size_of::<ShaderCommonUniform>() as _),
            },
            None,
        );

        if mode == RendererMode::GPUPowered {
            MaterialManager::add_to_bgl_gpu::<PbrMaterial>(&mut bglb);
        }

        let bulk_bgl = bglb.build(device, Some("bulk bglb"));

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

        let skybox_bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("skybox bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::Cube,
                    multisampled: false,
                },
                count: None,
            }],
        });

        Self {
            bulk_bgl,
            blit_bgl,
            skybox_bgl,
        }
    }
}
