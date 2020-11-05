use crate::datatypes::TextureHandle;
use glam::Vec2;

pub enum ResourceBinding<'a> {
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
    /// Usable in all modes.
    ///
    /// Each given texture will be it's own binding
    Custom2DTexture(&'a [TextureHandle]),
    /// Usable in all modes.
    ///
    /// Each given texture will be it's own binding
    CustomCubeTexture(&'a [TextureHandle]),
}

pub type ImageFormat = wgpu::TextureFormat;

pub enum ImageReference<'a> {
    SwapchainFrame,
    Custom(&'a str),
}

pub enum ImageResource<'a> {
    SwapchainFrame,
    Custom(ImageResourceDescriptor<'a>),
}

pub enum ImageResolution<'a> {
    /// Will be created at the given resolution. Must be a multiple
    /// of the block size.
    Fixed([u32; 2]),
    /// Floating point factor of another resource. Will be rounded
    /// to nearest multiple of block size.
    Relative(ImageReference<'a>, Vec2),
}

pub struct ImageResourceDescriptor<'a> {
    pub identifier: &'a str,
    pub resolution: [u32; 2],
    pub format: ImageFormat,
}

pub enum BufferReference<'a> {
    Custom(&'a str),
}

pub enum BufferResource<'a> {
    Custom(BufferResourceDescriptor<'a>),
}

pub struct BufferResourceDescriptor<'a> {
    pub identifier: &'a str,
    pub size: usize,
}
