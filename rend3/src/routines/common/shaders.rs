use wgpu::{Device, ShaderModule, ShaderModuleDescriptor, ShaderModuleDescriptorSpirV};

use crate::{shaders::SPIRV_SHADERS, RendererMode};

pub unsafe fn mode_safe_shader(device: &Device, mode: RendererMode, label: &str, cpu_source: &str, gpu_source: &str, unsafe_override: bool) -> ShaderModule {
    let source = SPIRV_SHADERS
        .get_file(match mode {
            RendererMode::CPUPowered => cpu_source,
            RendererMode::GPUPowered => gpu_source,
        })
        .unwrap()
        .contents();

    let use_unsafe = mode == RendererMode::GPUPowered || unsafe_override;

    match use_unsafe {
        false => device.create_shader_module(&ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::util::make_spirv(source),
        }),
        true => device.create_shader_module_spirv(&ShaderModuleDescriptorSpirV {
            label: Some(label),
            source: wgpu::util::make_spirv_raw(source),
        }),
    }
}
