use glam::{
    f32::{Vec3A, Vec4},
    Mat4, Vec2, Vec3,
};
use smallvec::SmallVec;
use std::num::NonZeroU32;
use wgpu::TextureFormat;

macro_rules! declare_handle {
    ($($name:ident),*) => {$(
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        pub struct $name(pub(crate) usize);

        impl $name {
            pub fn get(&self) -> usize {
                self.0
            }
        }
    )*};
}

declare_handle!(MeshHandle, TextureHandle, MaterialHandle, ObjectHandle);

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
    pub material: u32,  // 36..40
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
}

impl RendererTextureFormat {
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            RendererTextureFormat::Rgba8Srgb | RendererTextureFormat::Rgba8Linear => 4,
        }
    }
}

impl From<RendererTextureFormat> for wgpu::TextureFormat {
    fn from(other: RendererTextureFormat) -> Self {
        match other {
            RendererTextureFormat::Rgba8Srgb => TextureFormat::Rgba8UnormSrgb,
            RendererTextureFormat::Rgba8Linear => TextureFormat::Rgba8Unorm,
        }
    }
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
    pub(crate) struct AlbedoFlags : u32 {
        const ACTIVE = 0b001;
        const BLEND = 0b010;
        const VERT_SRGB = 0b100;
        const BLEND_SRGB = Self::BLEND.bits | Self::VERT_SRGB.bits;
    }
}

#[derive(Debug, Clone)]
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

    pub(crate) fn to_flags(&self) -> AlbedoFlags {
        match *self {
            Self::None => AlbedoFlags::empty(),
            Self::Vertex { srgb: false } => AlbedoFlags::ACTIVE | AlbedoFlags::VERT_SRGB,
            Self::Vertex { srgb: true } | Self::Value(_) | Self::Texture(_) => AlbedoFlags::ACTIVE,
            Self::ValueVertex { srgb: false, .. } | Self::TextureVertex { srgb: false, .. } => {
                AlbedoFlags::ACTIVE | AlbedoFlags::BLEND
            }
            Self::ValueVertex { srgb: true, .. } | Self::TextureVertex { srgb: true, .. } => {
                AlbedoFlags::ACTIVE | AlbedoFlags::BLEND_SRGB
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

#[derive(Debug, Clone)]
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
#[derive(Debug, Default, Clone)]
pub struct Material {
    pub albedo: AlbedoComponent,
    pub normal: Option<TextureHandle>,
    pub roughness: MaterialComponent<f32>,
    pub metallic: MaterialComponent<f32>,
    pub reflectance: MaterialComponent<f32>,
    pub clear_coat: MaterialComponent<f32>,
    pub clear_coat_roughness: MaterialComponent<f32>,
    pub anisotropy: MaterialComponent<f32>,
}

#[derive(Debug, Clone)]
pub struct Object {
    pub mesh: MeshHandle,
    pub materials: SmallVec<[MaterialHandle; 4]>,
    pub transform: AffineTransform,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct CameraLocation {
    pub location: Vec3A,
    pub pitch: f32,
    pub yaw: f32,
}
