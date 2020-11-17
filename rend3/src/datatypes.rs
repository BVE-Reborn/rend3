use crate::list::{ImageFormat, RenderPassSetRunRate};
use glam::{
    f32::{Vec3A, Vec4},
    Mat4, Vec2, Vec3,
};
use std::num::NonZeroU32;
use wgpu::TextureFormat;

#[macro_export]
#[doc(hidden)]
macro_rules! declare_handle {
    ($($name:ident),*) => {$(
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
        pub struct $name(pub(crate) usize);

        impl $name {
            pub fn get(&self) -> usize {
                self.0
            }
        }
    )*};
}

declare_handle!(
    MeshHandle,
    TextureHandle,
    MaterialHandle,
    ObjectHandle,
    DirectionalLightHandle,
    ShaderHandle,
    PipelineHandle
);

// Consider:
//
// Bone weights!!!
// Lightmap UVs
// Spherical harmonics
// Baked light color
// A lot of renderers put the tangent vector in the vertex data, but you can calculate it in the pixel shader ezpz
// Maybe thiccness data for tree branches
// I'd consider putting everything you can into the vertex data structure. Vertex data is just per-vertex data, and a lot of things can be per-vertex
// Then you don't need a million 4K textures
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ModelVertex {
    pub position: Vec3, // 00..12
    pub normal: Vec3,   // 12..24
    pub uv: Vec2,       // 24..32
    pub color: [u8; 4], // 32..36
}

unsafe impl bytemuck::Zeroable for ModelVertex {}
unsafe impl bytemuck::Pod for ModelVertex {}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct AffineTransform {
    pub transform: Mat4,
}

unsafe impl bytemuck::Zeroable for AffineTransform {}
unsafe impl bytemuck::Pod for AffineTransform {}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RendererTextureFormat {
    Rgba8Srgb,
    Rgba8Linear,
    Bc1Linear,
    Bc1Srgb,
    Bc3Linear,
    Bc3Srgb,
    Bc4Linear,
    Bc5Normal,
    Bc6Signed,
    Bc6Unsigned,
    Bc7Linear,
    Bc7Srgb,
}

impl RendererTextureFormat {
    pub fn pixels_per_block(&self) -> u32 {
        match self {
            RendererTextureFormat::Rgba8Srgb | RendererTextureFormat::Rgba8Linear => 1,
            RendererTextureFormat::Bc1Linear
            | RendererTextureFormat::Bc1Srgb
            | RendererTextureFormat::Bc3Linear
            | RendererTextureFormat::Bc3Srgb
            | RendererTextureFormat::Bc4Linear
            | RendererTextureFormat::Bc5Normal
            | RendererTextureFormat::Bc6Signed
            | RendererTextureFormat::Bc6Unsigned
            | RendererTextureFormat::Bc7Linear
            | RendererTextureFormat::Bc7Srgb => 4,
        }
    }

    pub fn bytes_per_block(&self) -> u32 {
        match self {
            RendererTextureFormat::Rgba8Srgb | RendererTextureFormat::Rgba8Linear => 4,
            RendererTextureFormat::Bc1Linear | RendererTextureFormat::Bc1Srgb | RendererTextureFormat::Bc4Linear => 8,
            RendererTextureFormat::Bc3Linear
            | RendererTextureFormat::Bc3Srgb
            | RendererTextureFormat::Bc5Normal
            | RendererTextureFormat::Bc6Signed
            | RendererTextureFormat::Bc6Unsigned
            | RendererTextureFormat::Bc7Linear
            | RendererTextureFormat::Bc7Srgb => 16,
        }
    }
}

impl From<RendererTextureFormat> for wgpu::TextureFormat {
    fn from(other: RendererTextureFormat) -> Self {
        match other {
            RendererTextureFormat::Rgba8Linear => TextureFormat::Rgba8Unorm,
            RendererTextureFormat::Rgba8Srgb => TextureFormat::Rgba8UnormSrgb,
            RendererTextureFormat::Bc1Linear => TextureFormat::Bc1RgbaUnorm,
            RendererTextureFormat::Bc1Srgb => TextureFormat::Bc1RgbaUnormSrgb,
            RendererTextureFormat::Bc3Linear => TextureFormat::Bc3RgbaUnorm,
            RendererTextureFormat::Bc3Srgb => TextureFormat::Bc3RgbaUnormSrgb,
            RendererTextureFormat::Bc4Linear => TextureFormat::Bc4RUnorm,
            RendererTextureFormat::Bc5Normal => TextureFormat::Bc5RgUnorm,
            RendererTextureFormat::Bc6Signed => TextureFormat::Bc6hRgbSfloat,
            RendererTextureFormat::Bc6Unsigned => TextureFormat::Bc6hRgbUfloat,
            RendererTextureFormat::Bc7Linear => TextureFormat::Bc7RgbaUnorm,
            RendererTextureFormat::Bc7Srgb => TextureFormat::Bc7RgbaUnormSrgb,
        }
    }
}

macro_rules! changeable_struct {
    ($(#[$outer:meta])* pub struct $name:ident <- nodefault $name_change:ident { $($field_vis:vis $field_name:ident : $field_type:ty),* $(,)? } ) => {
        $(#[$outer])*
        pub struct $name {
            $(
                $field_vis $field_name : $field_type
            ),*
        }
        impl $name {
            pub fn update_from_changes(&mut self, change: $name_change) {
                $(
                    if let Some(inner) = change.$field_name {
                        self.$field_name = inner;
                    }
                );*
            }
        }
        $(#[$outer])*
        pub struct $name_change {
            $(
                $field_vis $field_name : Option<$field_type>
            ),*
        }
    };
    ($(#[$outer:meta])* pub struct $name:ident <- $name_change:ident { $($field_vis:vis $field_name:ident : $field_type:ty),* $(,)? } ) => {
        $(#[$outer])*
        pub struct $name {
            $(
                $field_vis $field_name : $field_type
            ),*
        }
        impl $name {
            pub fn update_from_changes(&mut self, change: $name_change) {
                $(
                    if let Some(inner) = change.$field_name {
                        self.$field_name = inner;
                    }
                );*
            }
        }
        $(#[$outer])*
        #[derive(Default)]
        pub struct $name_change {
            $(
                $field_vis $field_name : Option<$field_type>
            ),*
        }
    };
}

#[derive(Debug, Clone)]
pub struct Mesh {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    pub material_count: u32,
    // TODO: Bones/joints/animation
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub data: Vec<u8>,
    pub format: RendererTextureFormat,
    pub width: u32,
    pub height: u32,
    pub label: Option<String>,
}

bitflags::bitflags! {
    pub(crate) struct MaterialFlags : u32 {
        const ALBEDO_ACTIVE = 0b000_001;
        const ALBEDO_BLEND = 0b000_010;
        const ALBEDO_VERTEX_SRGB = 0b000_100;
        const ALPHA_CUTOUT = 0b001_000;
        const BICOMPONENT_NORMAL = 0b010_000;
    }
}

#[derive(Debug, Copy, Clone)]
pub enum AlbedoComponent {
    /// No albedo color
    None,
    /// Albedo color is the vertex value
    Vertex {
        /// Vertex should be converted from srgb -> linear before multiplication
        srgb: bool,
    },
    /// Albedo color is the given value
    Value(Vec4),
    /// Albedo color is the given value multiplied by the vertex color
    ValueVertex {
        value: Vec4,
        /// Vertex should be converted from srgb -> linear before multiplication
        srgb: bool,
    },
    /// Albedo color is loaded from the given texture
    Texture(TextureHandle),
    /// Albedo color is loaded from the given texture, then multiplied
    /// by the vertex color;
    TextureVertex {
        handle: TextureHandle,
        /// Vertex should be converted from srgb -> linear before multiplication
        srgb: bool,
    },
}

impl Default for AlbedoComponent {
    fn default() -> Self {
        Self::None
    }
}

impl AlbedoComponent {
    pub(crate) fn to_value(&self) -> Vec4 {
        match *self {
            Self::Value(value) => value,
            Self::ValueVertex { value, .. } => value,
            _ => Vec4::default(),
        }
    }

    pub(crate) fn to_flags(&self) -> MaterialFlags {
        match *self {
            Self::None => MaterialFlags::empty(),
            Self::Vertex { srgb: false } => MaterialFlags::ALBEDO_ACTIVE | MaterialFlags::ALBEDO_VERTEX_SRGB,
            Self::Vertex { srgb: true } | Self::Value(_) | Self::Texture(_) => MaterialFlags::ALBEDO_ACTIVE,
            Self::ValueVertex { srgb: false, .. } | Self::TextureVertex { srgb: false, .. } => {
                MaterialFlags::ALBEDO_ACTIVE | MaterialFlags::ALBEDO_BLEND
            }
            Self::ValueVertex { srgb: true, .. } | Self::TextureVertex { srgb: true, .. } => {
                MaterialFlags::ALBEDO_ACTIVE | MaterialFlags::ALBEDO_BLEND | MaterialFlags::ALBEDO_VERTEX_SRGB
            }
        }
    }

    pub(crate) fn to_texture<Func>(&self, func: Func) -> Option<NonZeroU32>
    where
        Func: FnOnce(TextureHandle) -> NonZeroU32,
    {
        match *self {
            Self::None | Self::Vertex { .. } | Self::Value(_) | Self::ValueVertex { .. } => None,
            Self::Texture(handle) | Self::TextureVertex { handle, .. } => Some(func(handle)),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum MaterialComponent<T> {
    None,
    Value(T),
    Texture(TextureHandle),
}

impl<T> Default for MaterialComponent<T> {
    fn default() -> Self {
        Self::None
    }
}

impl<T: Copy> MaterialComponent<T> {
    pub(crate) fn to_value(&self, default: T) -> T {
        match *self {
            Self::Value(value) => value,
            Self::None | Self::Texture(_) => default,
        }
    }

    pub(crate) fn to_texture<Func>(&self, func: Func) -> Option<NonZeroU32>
    where
        Func: FnOnce(TextureHandle) -> NonZeroU32,
    {
        match *self {
            Self::None | Self::Value(_) => None,
            Self::Texture(texture) => Some(func(texture)),
        }
    }
}

// Consider:
//
// - Green screen value
changeable_struct! {
    #[derive(Debug, Default,Copy,  Clone)]
    pub struct Material <- nodefault MaterialChange {
        pub albedo: AlbedoComponent,
        pub normal: Option<TextureHandle>,
        pub roughness: MaterialComponent<f32>,
        pub metallic: MaterialComponent<f32>,
        pub reflectance: MaterialComponent<f32>,
        pub clear_coat: MaterialComponent<f32>,
        pub clear_coat_roughness: MaterialComponent<f32>,
        pub anisotropy: MaterialComponent<f32>,
        pub ambient_occlusion: MaterialComponent<f32>,
        pub alpha_cutout: Option<f32>,
    }
}

#[derive(Debug, Clone)]
pub struct Object {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    pub transform: AffineTransform,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct CameraLocation {
    pub location: Vec3A,
    pub pitch: f32,
    pub yaw: f32,
}

changeable_struct! {
    #[derive(Debug, Copy, Clone)]
    pub struct DirectionalLight <- DirectionalLightChange {
        pub color: Vec3,
        pub intensity: f32,
        pub direction: Vec3,
    }
}

pub enum PipelineInputType {
    FullscreenTriangle,
    Models3d,
}

pub enum PipelineBindingType {
    GeneralData,
    ObjectData,
    Material,
    CameraData,
    GPU2DTextures,
    GPUCubeTextures,
    ShadowTexture,
    SkyboxTexture,
    Custom2DTexture { count: usize },
    CustomCubeTexture { count: usize },
}

pub struct Pipeline {
    // TODO: Alpha
    pub run_rate: RenderPassSetRunRate,
    pub input: PipelineInputType,
    pub outputs: Vec<ImageFormat>,
    pub depth: Option<ImageFormat>,
    pub vertex: ShaderHandle,
    pub fragment: Option<ShaderHandle>,
    pub bindings: Vec<PipelineBindingType>,
}
