use crate::{LimitType, RendererInitializationError};
use wgpu::{BufferAddress, Features, Limits};

pub const MAX_UNIFORM_BUFFER_BINDING_SIZE: BufferAddress = 1 << 16; // Guaranteed on DI hardware

// This is a macro as bitflags are just totally not const
#[allow(non_snake_case)]
macro_rules! REQUIRED_FEATURES {
    () => {
        wgpu::Features::MAPPABLE_PRIMARY_BUFFERS
            | wgpu::Features::PUSH_CONSTANTS
            | wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::SAMPLED_TEXTURE_BINDING_ARRAY
            | wgpu::Features::SAMPLED_TEXTURE_ARRAY_DYNAMIC_INDEXING
            | wgpu::Features::SAMPLED_TEXTURE_ARRAY_NON_UNIFORM_INDEXING
    };
}

pub fn check_features(device: Features) -> Result<Features, RendererInitializationError> {
    let required = REQUIRED_FEATURES!();
    let missing = required - device;
    if !missing.is_empty() {
        Err(RendererInitializationError::MissingDeviceFeatures { features: missing })
    } else {
        Ok(required)
    }
}

const REQUIRED_LIMITS: Limits = Limits {
    max_bind_groups: 4,
    max_dynamic_uniform_buffers_per_pipeline_layout: 0,
    max_dynamic_storage_buffers_per_pipeline_layout: 0,
    max_sampled_textures_per_shader_stage: 128,
    max_samplers_per_shader_stage: 1,
    max_storage_buffers_per_shader_stage: 0,
    max_storage_textures_per_shader_stage: 0,
    max_uniform_buffers_per_shader_stage: 1,
    max_uniform_buffer_binding_size: MAX_UNIFORM_BUFFER_BINDING_SIZE as u32,
    max_push_constant_size: 128,
};

fn check_limit(d: u32, r: u32, ty: LimitType) -> Result<u32, RendererInitializationError> {
    if d < r {
        Err(RendererInitializationError::LowDeviceLimit {
            ty,
            device_limit: d,
            required_limit: r,
        })
    } else {
        Ok(r)
    }
}

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

#[must_use]
pub fn check_limits(device_limits: Limits) -> Result<Limits, RendererInitializationError> {
    let required_limits = REQUIRED_LIMITS;
    Ok(Limits {
        max_bind_groups: check_limit(
            device_limits.max_bind_groups,
            required_limits.max_bind_groups,
            LimitType::BindGroups,
        )?,
        max_dynamic_uniform_buffers_per_pipeline_layout: check_limit(
            device_limits.max_dynamic_uniform_buffers_per_pipeline_layout,
            required_limits.max_dynamic_uniform_buffers_per_pipeline_layout,
            LimitType::DynamicUniformBuffersPerPipelineLayout,
        )?,
        max_dynamic_storage_buffers_per_pipeline_layout: check_limit(
            device_limits.max_dynamic_storage_buffers_per_pipeline_layout,
            required_limits.max_dynamic_storage_buffers_per_pipeline_layout,
            LimitType::DynamicStorageBuffersPerPipelineLayout,
        )?,
        max_sampled_textures_per_shader_stage: check_limit_unlimited(
            device_limits.max_sampled_textures_per_shader_stage,
            required_limits.max_sampled_textures_per_shader_stage,
            LimitType::SampledTexturesPerShaderStage,
        )?,
        max_samplers_per_shader_stage: check_limit(
            device_limits.max_samplers_per_shader_stage,
            required_limits.max_samplers_per_shader_stage,
            LimitType::SamplersPerShaderStage,
        )?,
        max_storage_buffers_per_shader_stage: check_limit(
            device_limits.max_storage_buffers_per_shader_stage,
            required_limits.max_storage_buffers_per_shader_stage,
            LimitType::StorageBuffersPerShaderStage,
        )?,
        max_storage_textures_per_shader_stage: check_limit(
            device_limits.max_storage_textures_per_shader_stage,
            required_limits.max_storage_textures_per_shader_stage,
            LimitType::StorageTexturesPerShaderStage,
        )?,
        max_uniform_buffers_per_shader_stage: check_limit(
            device_limits.max_uniform_buffers_per_shader_stage,
            required_limits.max_uniform_buffers_per_shader_stage,
            LimitType::StorageTexturesPerShaderStage,
        )?,
        max_uniform_buffer_binding_size: check_limit(
            device_limits.max_uniform_buffer_binding_size,
            required_limits.max_uniform_buffer_binding_size,
            LimitType::UniformBufferBindingSize,
        )?,
        max_push_constant_size: check_limit(
            device_limits.max_push_constant_size,
            required_limits.max_push_constant_size,
            LimitType::PushConstantSize,
        )?,
    })
}
