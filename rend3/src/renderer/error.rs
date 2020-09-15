use thiserror::Error;
use wgpu::Features;

#[derive(Debug)]
pub enum LimitType {
    BindGroups,
    DynamicUniformBuffersPerPipelineLayout,
    DynamicStorageBuffersPerPipelineLayout,
    SampledTexturesPerShaderStage,
    SamplersPerShaderStage,
    StorageBuffersPerShaderStage,
    StorageTexturesPerShaderStage,
    UniformBuffersPerShaderStage,
    UniformBufferBindingSize,
    PushConstantSize,
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
