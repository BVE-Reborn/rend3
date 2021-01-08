use crate::{LimitType, RendererInitializationError, RendererMode};
use wgpu::{BufferAddress, Features, Limits};

pub const MAX_UNIFORM_BUFFER_BINDING_SIZE: BufferAddress = 1024;

pub fn gpu_required_features() -> Features {
    wgpu::Features::MAPPABLE_PRIMARY_BUFFERS
        | wgpu::Features::PUSH_CONSTANTS
        | wgpu::Features::TEXTURE_COMPRESSION_BC
        // | wgpu::Features::DEPTH_CLAMPING
        | wgpu::Features::SAMPLED_TEXTURE_BINDING_ARRAY
        | wgpu::Features::SAMPLED_TEXTURE_ARRAY_DYNAMIC_INDEXING
        | wgpu::Features::SAMPLED_TEXTURE_ARRAY_NON_UNIFORM_INDEXING
        | wgpu::Features::UNSIZED_BINDING_ARRAY
        | wgpu::Features::MULTI_DRAW_INDIRECT
        | wgpu::Features::MULTI_DRAW_INDIRECT_COUNT
}

pub fn cpu_required_features() -> Features {
    wgpu::Features::MAPPABLE_PRIMARY_BUFFERS | wgpu::Features::PUSH_CONSTANTS
}

pub fn optional_features() -> Features {
    wgpu::Features::TEXTURE_COMPRESSION_BC
}

pub fn check_features(mode: RendererMode, device: Features) -> Result<Features, RendererInitializationError> {
    let required = match mode {
        RendererMode::GPUPowered => gpu_required_features(),
        RendererMode::CPUPowered => cpu_required_features(),
    };
    let optional = optional_features() & device;
    let missing = required - device;
    if !missing.is_empty() {
        Err(RendererInitializationError::MissingDeviceFeatures { features: missing })
    } else {
        Ok(required | optional)
    }
}

pub const GPU_REQUIRED_LIMITS: Limits = Limits {
    max_bind_groups: 8,
    max_dynamic_uniform_buffers_per_pipeline_layout: 0,
    max_dynamic_storage_buffers_per_pipeline_layout: 0,
    max_sampled_textures_per_shader_stage: 256,
    max_samplers_per_shader_stage: 2,
    max_storage_buffers_per_shader_stage: 5,
    max_storage_textures_per_shader_stage: 0,
    max_uniform_buffers_per_shader_stage: 2,
    max_uniform_buffer_binding_size: MAX_UNIFORM_BUFFER_BINDING_SIZE as u32,
    max_push_constant_size: 128,
};

pub const CPU_REQUIRED_LIMITS: Limits = Limits {
    max_bind_groups: 8,
    max_dynamic_uniform_buffers_per_pipeline_layout: 0,
    max_dynamic_storage_buffers_per_pipeline_layout: 0,
    max_sampled_textures_per_shader_stage: 10,
    max_samplers_per_shader_stage: 2,
    max_storage_buffers_per_shader_stage: 2,
    max_storage_textures_per_shader_stage: 0,
    max_uniform_buffers_per_shader_stage: 2,
    max_uniform_buffer_binding_size: MAX_UNIFORM_BUFFER_BINDING_SIZE as u32,
    max_push_constant_size: 128,
};

fn check_limit_unlimited(d: u32, r: u32, ty: LimitType) -> Result<u32, RendererInitializationError> {
    if d < r {
        Err(RendererInitializationError::LowDeviceLimit {
            ty,
            device_limit: d,
            required_limit: r,
        })
    } else {
        Ok(d)
    }
}

pub fn check_limits(mode: RendererMode, device_limits: &Limits) -> Result<Limits, RendererInitializationError> {
    let required_limits = match mode {
        RendererMode::GPUPowered => GPU_REQUIRED_LIMITS,
        RendererMode::CPUPowered => CPU_REQUIRED_LIMITS,
    };
    Ok(Limits {
        max_bind_groups: check_limit_unlimited(
            device_limits.max_bind_groups,
            required_limits.max_bind_groups,
            LimitType::BindGroups,
        )?,
        max_dynamic_uniform_buffers_per_pipeline_layout: check_limit_unlimited(
            device_limits.max_dynamic_uniform_buffers_per_pipeline_layout,
            required_limits.max_dynamic_uniform_buffers_per_pipeline_layout,
            LimitType::DynamicUniformBuffersPerPipelineLayout,
        )?,
        max_dynamic_storage_buffers_per_pipeline_layout: check_limit_unlimited(
            device_limits.max_dynamic_storage_buffers_per_pipeline_layout,
            required_limits.max_dynamic_storage_buffers_per_pipeline_layout,
            LimitType::DynamicStorageBuffersPerPipelineLayout,
        )?,
        max_sampled_textures_per_shader_stage: check_limit_unlimited(
            device_limits.max_sampled_textures_per_shader_stage,
            required_limits.max_sampled_textures_per_shader_stage,
            LimitType::SampledTexturesPerShaderStage,
        )?,
        max_samplers_per_shader_stage: check_limit_unlimited(
            device_limits.max_samplers_per_shader_stage,
            required_limits.max_samplers_per_shader_stage,
            LimitType::SamplersPerShaderStage,
        )?,
        max_storage_buffers_per_shader_stage: check_limit_unlimited(
            device_limits.max_storage_buffers_per_shader_stage,
            required_limits.max_storage_buffers_per_shader_stage,
            LimitType::StorageBuffersPerShaderStage,
        )?,
        max_storage_textures_per_shader_stage: check_limit_unlimited(
            device_limits.max_storage_textures_per_shader_stage,
            required_limits.max_storage_textures_per_shader_stage,
            LimitType::StorageTexturesPerShaderStage,
        )?,
        max_uniform_buffers_per_shader_stage: check_limit_unlimited(
            device_limits.max_uniform_buffers_per_shader_stage,
            required_limits.max_uniform_buffers_per_shader_stage,
            LimitType::StorageTexturesPerShaderStage,
        )?,
        max_uniform_buffer_binding_size: check_limit_unlimited(
            device_limits.max_uniform_buffer_binding_size,
            required_limits.max_uniform_buffer_binding_size,
            LimitType::UniformBufferBindingSize,
        )?,
        max_push_constant_size: check_limit_unlimited(
            device_limits.max_push_constant_size,
            required_limits.max_push_constant_size,
            LimitType::PushConstantSize,
        )?,
    })
}
