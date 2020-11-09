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
    pub outputs: Vec<ImageOutput>,
    pub depth: Option<ImageOutputReference>,
}

pub struct RenderOpDescriptor {
    pub input: RenderOpInputType,
    pub vertex: String,
    pub fragment: Option<String>,
    pub bindings: Vec<ResourceBinding>,
}

pub enum RenderOpInputType {
    /// No bound vertex inputs, just a simple `draw(0..3)`
    FullscreenTriangle,
    /// Render all 3D models.
    // TODO: Filtering
    Models3D,
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
    pub stage: ShaderStage,
    pub includes: Vec<String>,
    pub defines: Vec<(String, Option<String>)>,
}

pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
}
