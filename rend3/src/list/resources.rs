use crate::datatypes::TextureHandle;

pub enum ResourceBinding {
    /// Bindings in All Modes:
    /// 0: Linear Sampler
    /// 1: Shadow Sampler
    GeneralData,
    /// Bindings in All Modes:
    /// 0: Object Data buffer
    ObjectData,
    /// Bindings in GPU-Powered Mode:
    /// 0: Material Buffer
    ///
    /// Bindings in CPU-Powered Mode:
    /// 0: Albedo Texture
    /// 1: Normal Texture
    /// 2: Roughness Texture
    /// 3: Metallic Texture
    /// 4: Reflectance Texture
    /// 5: Clear Coat Texture
    /// 6: Clear Coat Roughness Texture
    /// 7: Anisotropy Texture
    /// 8: Ambient Occlusion Texture
    /// 9: Texture Data
    Material,
    /// Bindings in All Modes:
    /// 0: Camera Data Uniform Buffer
    CameraData,
    /// May only be bound in GPU-powered mode:
    /// 0: 2D Texture Array
    GPU2DTextures,
    /// May only be bound in GPU-powered mode:
    /// 0: Cubemap Texture Array
    GPUCubeTextures,
    /// Bindings in All Modes:
    /// 0: Shadow `texture2DArray`
    ShadowTexture,
    /// Binding in All Modes:
    /// 0: Current skybox texture
    SkyboxTexture,
    /// Usable in all modes.
    ///
    /// Each given texture will be it's own binding
    Custom2DTexture(Vec<ImageInputReference>),
    /// Usable in all modes.
    ///
    /// Each given texture will be it's own binding
    CustomCubeTexture(Vec<ImageInputReference>),
}

pub type ImageFormat = wgpu::TextureFormat;
pub type ImageUsage = wgpu::TextureUsage;
pub type BufferUsage = wgpu::BufferUsage;

pub enum ImageReference {
    OutputImage,
    Handle(TextureHandle),
    Custom(String),
}

pub enum ImageInputReference {
    Handle(TextureHandle),
    Custom(String),
}

pub struct ImageOutput {
    pub output: ImageOutputReference,
    pub resolve_target: Option<ImageOutputReference>,
}

pub enum ImageOutputReference {
    OutputImage,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageResourceDescriptor {
    pub resolution: [u32; 2],
    pub format: ImageFormat,
    pub samples: u32,
    pub usage: ImageUsage,
}

pub enum BufferReference<'a> {
    Custom(&'a str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferResourceDescriptor {
    pub size: usize,
    pub usage: BufferUsage,
}
