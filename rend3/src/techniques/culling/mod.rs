use crevice::std430::AsStd430;

use crate::{datatypes::MaterialHandle, util::math::IndexedDistance, ModeData};

mod cpu;
mod gpu;

struct CulledObjectSet {
    data: ModeData<CpuCulledObjectSet, GpuCulledObjectSet>,
}

struct CpuCulledObjectSet {
    call: Vec<CPUDrawCall>,
    output: Vec<ShaderOutputObject>,
    distance: Vec<IndexedDistance>,
}

#[derive(Debug, Copy, Clone)]
struct CPUDrawCall {
    pub start_idx: u32,
    pub count: u32,
    pub vertex_offset: i32,
    pub handle: MaterialHandle,
}

#[derive(Debug, Copy, Clone, AsStd430)]
struct ShaderOutputObject {
    model_view: mint::ColumnMatrix4<f32>,
    model_view_proj: mint::ColumnMatrix4<f32>,
    // Actually a mat3, but funky shader time
    inv_trans_model_view: mint::ColumnMatrix3<f32>,
    // Unused in shader
    _material_idx: u32,
    // Unused in shader
    _active: u32,
}

struct GpuCulledObjectSet {}
