use rend3::ModeData;
use wgpu::Buffer;

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
    pub material_index: u32,
}
