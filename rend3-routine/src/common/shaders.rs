use std::borrow::Cow;

use rend3::RendererProfile;
use wgpu::{Device, ShaderModule, ShaderModuleDescriptor, ShaderModuleDescriptorSpirV, ShaderSource};

use crate::shaders::{SPIRV_SHADERS, WGSL_SHADERS};

/// When CpuDriven, creates a checked wgsl shader, when CpuDriven creates a
/// passthrough SPIRV shader.
///
/// # Safety
///
/// The shader must be valid, match all the respective definitions, and
/// otherwise meet wgpu's validation requirements
pub unsafe fn profile_safe_shader(
    device: &Device,
    profile: RendererProfile,
    label: &str,
    cpu_source: &str,
    gpu_source: &str,
) -> ShaderModule {
    let shader_dir = match profile {
        RendererProfile::CpuDriven => &WGSL_SHADERS,
        RendererProfile::GpuDriven => &SPIRV_SHADERS,
    };

    let source = shader_dir
        .get_file(match profile {
            RendererProfile::CpuDriven => cpu_source,
            RendererProfile::GpuDriven => gpu_source,
        })
        .unwrap()
        .contents();

    let use_unsafe = profile == RendererProfile::GpuDriven;

    match use_unsafe {
        false => device.create_shader_module(&ShaderModuleDescriptor {
            label: Some(label),
            source: ShaderSource::Wgsl(Cow::Borrowed(std::str::from_utf8(source).unwrap())),
        }),
        true => device.create_shader_module_spirv(&ShaderModuleDescriptorSpirV {
            label: Some(label),
            source: wgpu::util::make_spirv_raw(source),
        }),
    }
}
