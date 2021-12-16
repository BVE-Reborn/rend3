use thiserror::Error;
use wgpu::Features;

/// Enum mapping to each of a device's limit.
#[derive(Debug)]
pub enum LimitType {
    BindGroups,
    DynamicUniformBuffersPerPipelineLayout,
    DynamicStorageBuffersPerPipelineLayout,
    SampledTexturesPerShaderStages,
    SamplersPerShaderStages,
    StorageBuffersPerShaderStages,
    StorageTexturesPerShaderStages,
    UniformBuffersPerShaderStages,
    UniformBufferBindingSize,
    UniformBufferBindingAlignment,
    StorageBufferBindingAlignment,
    PushConstantSize,
    MaxTextureDimension1d,
    MaxTextureDimension2d,
    MaxTextureDimension3d,
    MaxTextureArrayLayers,
    MaxStorageBufferBindingSize,
    MaxVertexBuffers,
    MaxVertexAttributes,
    MaxVertexBufferArrayStride,
    MaxInterStageShaderComponents,
    MaxComputeWorkgroupStorageSize,
    MaxComputeInvocationsPerWorkgroup,
    MaxComputeWorkgroupSizeX,
    MaxComputeWorkgroupSizeY,
    MaxComputeWorkgroupSizeZ,
    MaxComputeWorkgroupsPerDimension,
}

/// Reason why the renderer failed to initialize.
#[derive(Error, Debug)]
pub enum RendererInitializationError {
    #[error("No supported adapter found")]
    MissingAdapter,
    #[error(
        "The device limit of {:?} is {} but renderer requires at least {}",
        ty,
        device_limit,
        required_limit
    )]
    LowDeviceLimit {
        ty: LimitType,
        device_limit: u32,
        required_limit: u32,
    },
    #[error("Device is missing required features: {:?}", features)]
    MissingDeviceFeatures { features: Features },
    #[error("Requesting a device failed")]
    RequestDeviceFailed,
}
