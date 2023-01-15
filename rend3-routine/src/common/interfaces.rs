use std::{marker::PhantomData, mem, num::NonZeroU64};

use glam::{Mat4, Vec3};
use rend3::{managers::DirectionalLightManager, types::Material, util::bind_merge::BindGroupLayoutBuilder};
use wgpu::{
    BindGroupLayout, BindingType, BufferBindingType, Device, ShaderStages, TextureSampleType, TextureViewDimension,
};

use crate::{common::samplers::Samplers, uniforms::FrameUniforms};

/// Interfaces which are used throughout the whole frame.
///
/// Contains the samplers, per frame uniforms, and directional light
/// information.
pub struct WholeFrameInterfaces {
    /// Includes everything excluding the directional light information to
    /// prevent cycles when rendering to shadow maps.
    pub depth_uniform_bgl: BindGroupLayout,
    /// Includes everything.
    pub forward_uniform_bgl: BindGroupLayout,
}

impl WholeFrameInterfaces {
    pub fn new(device: &Device) -> Self {
        profiling::scope!("ShaderInterfaces::new");

        let mut uniform_bglb = BindGroupLayoutBuilder::new();

        Samplers::add_to_bgl(&mut uniform_bglb);

        uniform_bglb.append(
            ShaderStages::VERTEX_FRAGMENT,
            BindingType::Buffer {
                ty: BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: NonZeroU64::new(mem::size_of::<FrameUniforms>() as _),
            },
            None,
        );

        DirectionalLightManager::add_to_bgl(&mut uniform_bglb);

        let shadow_uniform_bgl = uniform_bglb.build(device, Some("shadow uniform bgl"));

        // Shadow texture
        uniform_bglb.append(
            ShaderStages::FRAGMENT,
            BindingType::Texture {
                sample_type: TextureSampleType::Depth,
                view_dimension: TextureViewDimension::D2,
                multisampled: false,
            },
            None,
        );

        let forward_uniform_bgl = uniform_bglb.build(device, Some("forward uniform bgl"));

        Self {
            depth_uniform_bgl: shadow_uniform_bgl,
            forward_uniform_bgl,
        }
    }
}

/// The input structure that the culling shaders/functions output and drawing
/// shaders read.
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct PerObjectDataAbi {
    pub model_view: Mat4,
    pub model_view_proj: Mat4,
    // Only read when GpuDriven. Materials are directly bound when CpuDriven.
    pub material_idx: u32,
    pub pad0: [u8; 12],
    pub inv_squared_scale: Vec3,
}

unsafe impl bytemuck::Pod for PerObjectDataAbi {}
unsafe impl bytemuck::Zeroable for PerObjectDataAbi {}

/// Interface which has all per-material-archetype data: the object output
/// buffer and the gpu material buffer.
pub struct PerMaterialArchetypeInterface<M> {
    pub bgl: BindGroupLayout,
    _phantom: PhantomData<M>,
}
impl<M: Material> PerMaterialArchetypeInterface<M> {
    pub fn new(device: &Device) -> Self {
        let bgl = BindGroupLayoutBuilder::new()
            .append(
                ShaderStages::VERTEX_FRAGMENT,
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                None,
            )
            .append(
                ShaderStages::VERTEX_FRAGMENT,
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: true,
                    min_binding_size: None,
                },
                None,
            )
            .append(
                ShaderStages::VERTEX_FRAGMENT,
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                },
                None,
            )
            .append(
                ShaderStages::VERTEX_FRAGMENT,
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                None,
            )
            .build(device, Some("per material bgl"));

        Self {
            bgl,
            _phantom: PhantomData,
        }
    }
}
