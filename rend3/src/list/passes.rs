use crate::list::{ImageOutput, ImageOutputReference, ResourceBinding};

pub struct RenderPassSetDescriptor {
    pub run_rate: RenderPassSetRunRate,
}

pub enum RenderPassSetRunRate {
    /// Run this RenderPassSet once for every shadow, the output texture being the shadow map.
    PerShadow,
    /// Run this RenderPassSet once. Output texture is the swapchain frame.
    Once,
}

pub struct RenderPassDescriptor {
    pub vertex: ShaderSource,
    pub fragment: Option<ShaderSource>,
    pub bindings: Vec<ResourceBinding>,
    pub outputs: Vec<ImageOutput>,
    pub depth: Option<ImageOutputReference>,
}

pub enum ShaderSource {
    SpirV(Vec<u32>),
    Glsl(SourceShaderDescriptor),
}

pub enum ShaderSourceType {
    /// Load shader from given file
    File(String),
    /// Use given shader source
    Value(String),
}

pub struct SourceShaderDescriptor {
    pub source: ShaderSourceType,
    pub includes: Vec<String>,
    pub defines: Vec<(String, Option<String>)>,
}
