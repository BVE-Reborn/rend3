use crate::{
    datatypes::PipelineHandle,
    list::{DepthOutput, ImageOutput, PerObjectResourceBinding, ResourceBinding},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RenderPassRunRate {
    /// Run this RenderPassSet once for every shadow, the output texture being the shadow map.
    PerShadow,
    /// Run this RenderPassSet once. Output texture is the swapchain frame.
    Once,
}

#[derive(Debug, Clone)]
pub struct RenderPassDescriptor {
    pub run_rate: RenderPassRunRate,
    pub outputs: Vec<ImageOutput>,
    pub depth: Option<DepthOutput>,
}

#[derive(Debug, Clone)]
pub struct RenderOpDescriptor {
    pub pipeline: PipelineHandle,
    pub input: RenderOpInputType,
    pub per_op_bindings: Vec<ResourceBinding>,
    pub per_object_bindings: Vec<PerObjectResourceBinding>,
}

#[derive(Debug, Clone)]
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
    /// Load builtin shader
    Builtin(String),
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
