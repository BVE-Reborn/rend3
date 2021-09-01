use glam::Mat4;
use rend3::{types::RawMaterialHandle, util::frustum::BoundingSphere, ModeData};
use wgpu::{BindGroup, Buffer};

pub mod cpu;
pub mod gpu;

pub struct CulledObjectSet {
    pub calls: ModeData<Vec<CPUDrawCall>, GPUIndirectData>,
    pub output_bg: BindGroup,
}

pub struct GPUIndirectData {
    pub indirect_buffer: Buffer,
    pub count: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Sorting {
    FrontToBack,
    BackToFront,
}

#[derive(Debug, Clone)]
pub struct CPUDrawCall {
    pub start_idx: u32,
    pub end_idx: u32,
    pub vertex_offset: i32,
    pub material_handle: RawMaterialHandle,
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
