use thiserror::Error;
use wgpu::Features;
use wgpu_profiler::CreationError;

/// Enum mapping to each of a device's limit.
#[derive(Debug)]
pub enum LimitType {
    BindGroups,
    MaxBindingsPerBindGroup,
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
    MaxBufferSize,
    MaxColorAttachmentBytesPerSample,
    MaxColorAttachments,
    MaxSubgroupSize,
    MinSubgroupSize,
}

/// Reason why the renderer failed to initialize.
#[derive(Error, Debug)]
pub enum RendererInitializationError {
    #[error("No supported adapter found")]
    MissingAdapter,
    #[error("The device limit of {:?} is {} but renderer requires at least {}", ty, device_limit, required_limit)]
    LowDeviceLimit { ty: LimitType, device_limit: u64, required_limit: u64 },
    #[error("Device is missing required features: {:?}", features)]
    MissingDeviceFeatures { features: Features },
    #[error("Requesting a device failed")]
    RequestDeviceFailed,
    #[error("Failed to create GpuProfiler")]
    GpuProfilerCreation(#[source] CreationError),
}
