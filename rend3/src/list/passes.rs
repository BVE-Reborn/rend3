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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShaderSource {
    SpirV(Vec<u32>),
    Glsl(SourceShaderDescriptor),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ShaderSourceType {
    /// Load shader from given file
    File(String),
    /// Use given shader source
    Value(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceShaderDescriptor {
    pub source: ShaderSourceType,
    pub stage: ShaderSourceStage,
    pub includes: Vec<String>,
    pub defines: Vec<(String, Option<String>)>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ShaderSourceStage {
    Vertex,
    Fragment,
    Compute,
}

impl From<ShaderSourceStage> for shaderc::ShaderKind {
    fn from(stage: ShaderSourceStage) -> Self {
        match stage {
            ShaderSourceStage::Vertex => shaderc::ShaderKind::Vertex,
            ShaderSourceStage::Fragment => shaderc::ShaderKind::Fragment,
            ShaderSourceStage::Compute => shaderc::ShaderKind::Compute,
        }
    }
}
