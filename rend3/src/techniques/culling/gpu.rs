use std::borrow::Cow;

use wgpu::{ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, ShaderFlags, ShaderModuleDescriptor, ShaderSource, VertexState};

use crate::{cache::{BindGroupCache, PipelineCache, ShaderModuleCache}, shaders::SPIRV_SHADERS, techniques::culling::GpuCulledObjectSet};

pub(super) fn run(
    device: &Device,
    sm_cache: &mut ShaderModuleCache,
    pipeline_cache: &mut PipelineCache,
    bind_group_cache: &mut BindGroupCache,
) -> GpuCulledObjectSet {
    let sm = sm_cache.shader_module(device, &ShaderModuleDescriptor {
        label: Some("cull"),
        source: wgpu::util::make_spirv(SPIRV_SHADERS.get_file("cull.comp.spv").unwrap().contents()),
        flags: ShaderFlags::empty(),
    });

    let pipeline = pipeline_cache.compute_pipeline(
        device,
        Some("cull"),
        &PipelineLayoutDescriptor {
            label: Some("cull"),
            bind_group_layouts: &[todo!()],
            push_constant_ranges: &[],
        },
        &ComputePipelineDescriptor {
            label: Some("cull"),
            layout: None,
            entry_point: "main",
            module: &sm, 
        },
    );

    GpuCulledObjectSet {}
}
