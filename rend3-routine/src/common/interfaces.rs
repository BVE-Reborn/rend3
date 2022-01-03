use std::{mem, num::NonZeroU64};

use glam::{Mat4, Vec3};
use rend3::{
    managers::{DirectionalLightManager, MaterialManager},
    util::bind_merge::BindGroupLayoutBuilder,
    RendererMode,
};
use wgpu::{BindGroupLayout, BindingType, BufferBindingType, Device, ShaderStages};

use crate::{common::samplers::Samplers, material::PbrMaterial, uniforms::ShaderCommonUniform};

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct PerObjectData {
    pub model_view: Mat4,
    pub model_view_proj: Mat4,
    // Unused in shader
    pub material_idx: u32,
    pub pad0: [u8; 12],
    pub inv_squared_scale: Vec3,
}

unsafe impl bytemuck::Pod for PerObjectData {}
unsafe impl bytemuck::Zeroable for PerObjectData {}

pub struct ShaderInterfaces {
    pub shadow_uniform_bgl: BindGroupLayout,
    pub forward_uniform_bgl: BindGroupLayout,
    pub per_material_bgl: BindGroupLayout,
}

impl ShaderInterfaces {
    pub fn new(device: &Device, mode: RendererMode) -> Self {
        profiling::scope!("ShaderInterfaces::new");

        let mut uniform_bglb = BindGroupLayoutBuilder::new();

        Samplers::add_to_bgl(&mut uniform_bglb);

        uniform_bglb.append(
            ShaderStages::VERTEX_FRAGMENT,
            BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(mem::size_of::<ShaderCommonUniform>() as _),
            },
            None,
        );

        let shadow_uniform_bgl = uniform_bglb.build(device, Some("shadow uniform bgl"));

        DirectionalLightManager::add_to_bgl(&mut uniform_bglb);

        let forward_uniform_bgl = uniform_bglb.build(device, Some("forward uniform bgl"));

        let mut per_material_bglb = BindGroupLayoutBuilder::new();

        per_material_bglb.append(
            ShaderStages::VERTEX,
            BindingType::Buffer {
                ty: BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(mem::size_of::<PerObjectData>() as _),
            },
            None,
        );

        if mode == RendererMode::GPUPowered {
            MaterialManager::add_to_bgl_gpu::<PbrMaterial>(&mut per_material_bglb);
        }

        let per_material_bgl = per_material_bglb.build(device, Some("per material bgl"));

        Self {
            shadow_uniform_bgl,
            forward_uniform_bgl,
            per_material_bgl,
        }
    }
}
