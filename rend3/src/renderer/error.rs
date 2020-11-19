use crate::list::SourceShaderDescriptor;
use std::io;
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

#[derive(Error, Debug)]
pub enum ShaderError {
    #[error("IO error while loading shader {1:?}: {0}")]
    FileError(#[source] io::Error, SourceShaderDescriptor),
    #[error("Error locating a builtin shader {0:?}")]
    Builtin(SourceShaderDescriptor),
    #[error("Compilation error with shader args: {1:?}: {0}")]
    CompileError(#[source] shaderc::Error, SourceShaderDescriptor),
}
