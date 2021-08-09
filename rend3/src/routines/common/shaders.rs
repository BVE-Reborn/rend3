use wgpu::{Device, ShaderModule, ShaderModuleDescriptor, ShaderModuleDescriptorSpirV};

use crate::{shaders::SPIRV_SHADERS, RendererMode};

pub unsafe fn mode_safe_shader(device: &Device, mode: RendererMode, label: &str, cpu_source: &str, gpu_source: &str) -> ShaderModule {
    let source = SPIRV_SHADERS
        .get_file(match mode {
            RendererMode::CPUPowered => cpu_source,
            RendererMode::GPUPowered => gpu_source,
        })
        .unwrap()
        .contents();

    match mode {
        RendererMode::CPUPowered => device.create_shader_module(&ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::util::make_spirv(source),
        }),
        RendererMode::GPUPowered => device.create_shader_module_spirv(&ShaderModuleDescriptorSpirV {
            label: Some(label),
            source: wgpu::util::make_spirv_raw(source),
        }),
    }
}
