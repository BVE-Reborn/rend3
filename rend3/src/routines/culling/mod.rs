use crevice::{std140::AsStd140, std430::AsStd430};
use wgpu::Buffer;

use crate::{datatypes::MaterialHandle, util::math::IndexedDistance, ModeData};

mod cpu;
mod gpu;

struct CulledObjectSet {
    data: ModeData<CpuCulledObjectSet, GpuCulledObjectSet>,
}

struct CpuCulledObjectSet {
    call: Vec<CPUDrawCall>,
    distance: Vec<IndexedDistance>,
    output_buffer: Buffer,
}

struct GpuCulledObjectSet {
    indirect_buffer: Buffer,
    output_buffer: Buffer,
}

#[derive(Debug, Copy, Clone)]
struct CPUDrawCall {
    pub start_idx: u32,
    pub count: u32,
    pub vertex_offset: i32,
    pub handle: MaterialHandle,
}

#[derive(Debug, Copy, Clone, AsStd430)]
struct GPUCullingInput {
    start_idx: u32,
    count: u32,
    vertex_offset: i32,
    material_idx: u32,
    transform: mint::ColumnMatrix4<f32>,
    // xyz position; w radius
    bounding_sphere: mint::Vector4<f32>,
}

#[derive(Debug, Copy, Clone, AsStd430)]
struct CullingOutput {
    model_view: mint::ColumnMatrix4<f32>,
    model_view_proj: mint::ColumnMatrix4<f32>,
    inv_trans_model_view: mint::ColumnMatrix3<f32>,
    // Unused in shader
    material_idx: u32,
}
