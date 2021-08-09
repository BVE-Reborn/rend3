use thiserror::Error;
use wgpu::Features;

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
    PushConstantSize,
    MaxTextureDimension1d,
    MaxTextureDimension2d,
    MaxTextureDimension3d,
    MaxTextureArrayLayers,
    MaxStorageBufferBindingSize,
    MaxVertexBuffers,
    MaxVertexAttributes,
    MaxVertexBufferArrayStride,
}

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
