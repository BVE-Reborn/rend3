use glam::{Mat3A, Mat4};
use wgpu::Buffer;

use crate::{datatypes::MaterialHandle, util::frustum::BoundingSphere, ModeData};

pub mod cpu;
pub mod gpu;

pub struct CulledObjectSet {
    pub calls: ModeData<Vec<CPUDrawCall>, GPUIndirectData>,
    pub output_buffer: Buffer,
}

pub struct GPUIndirectData {
    pub indirect_buffer: Buffer,
    pub count: usize,
}

#[derive(Debug, Copy, Clone)]
pub struct CPUDrawCall {
    pub start_idx: u32,
    pub count: u32,
    pub vertex_offset: i32,
    pub handle: MaterialHandle,
}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct GPUCullingInput {
    pub start_idx: u32,
    pub count: u32,
    pub vertex_offset: i32,
    pub material_idx: u32,
    pub transform: Mat4,
    // xyz position; w radius
    pub bounding_sphere: BoundingSphere,
}

unsafe impl bytemuck::Pod for GPUCullingInput {}
unsafe impl bytemuck::Zeroable for GPUCullingInput {}

#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct CullingOutput {
    model_view: Mat4,
    model_view_proj: Mat4,
    inv_trans_model_view: Mat3A,
    // Unused in shader
    material_idx: u32,
}

unsafe impl bytemuck::Pod for CullingOutput {}
unsafe impl bytemuck::Zeroable for CullingOutput {}
