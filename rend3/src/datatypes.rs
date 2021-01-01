use crate::list::{ImageFormat, RenderPassRunRate};
use glam::{
    f32::{Vec3A, Vec4},
    Mat4, Vec2, Vec3,
};
use wgpu::TextureFormat;
pub use wgpu::{Color as ClearColor, LoadOp as PipelineLoadOp};

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
#[derive(Debug, Default, Copy, Clone)]
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

#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    // TODO: Bones/joints/animation
}

impl Mesh {
    pub fn calculate_normals(&mut self) {
        for vert in &mut self.vertices {
            vert.normal = Vec3::zero();
        }

        for idx in self.indices.chunks_exact(3) {
            let pos1 = self.vertices[idx[0] as usize].position;
            let pos2 = self.vertices[idx[1] as usize].position;
            let pos3 = self.vertices[idx[2] as usize].position;

            let edge1 = pos2 - pos1;
            let edge2 = pos3 - pos1;

            let normal = edge1.cross(edge2);

            self.vertices[idx[0] as usize].normal += normal;
            self.vertices[idx[1] as usize].normal += normal;
            self.vertices[idx[2] as usize].normal += normal;
        }

        for vert in &mut self.vertices {
            vert.normal = vert.normal.normalize();
        }
    }
}

#[derive(Debug, Clone)]
pub struct Texture {
    pub data: Vec<u8>,
    pub format: RendererTextureFormat,
    pub width: u32,
    pub height: u32,
    pub label: Option<String>,
    pub mip_levels: u32,
}

bitflags::bitflags! {
    pub(crate) struct MaterialFlags : u32 {
        const ALBEDO_ACTIVE =      0b0000_0000_0001;
        const ALBEDO_BLEND =       0b0000_0000_0010;
        const ALBEDO_VERTEX_SRGB = 0b0000_0000_0100;
        const ALPHA_CUTOUT =       0b0000_0000_1000;
        const BICOMPONENT_NORMAL = 0b0000_0001_0000;
        const SWIZZLED_NORMAL =    0b0000_0010_0000;
        const AOMR_GLTF_COMBINED = 0b0000_0100_0000;
        const AOMR_GLTF_SPLIT =    0b0000_1000_0000;
        const AOMR_BW_SPLIT =      0b0001_0000_0000;
        const CC_GLTF_COMBINED =   0b0010_0000_0000;
        const CC_GLTF_SPLIT =      0b0100_0000_0000;
        const CC_BW_SPLIT =        0b1000_0000_0000;
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
    /// Albedo color is loaded from given texture, then multiplied
    /// by the given value.
    TextureValue { handle: TextureHandle, value: Vec4 },
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
            Self::TextureValue { value, .. } => value,
            _ => Vec4::splat(1.0),
        }
    }

    pub(crate) fn to_flags(&self) -> MaterialFlags {
        match *self {
            Self::None => MaterialFlags::empty(),
            Self::Value(_) | Self::Texture(_) | Self::TextureValue { .. } => MaterialFlags::ALBEDO_ACTIVE,
            Self::Vertex { srgb: false }
            | Self::ValueVertex { srgb: false, .. }
            | Self::TextureVertex { srgb: false, .. } => MaterialFlags::ALBEDO_ACTIVE | MaterialFlags::ALBEDO_BLEND,
            Self::Vertex { srgb: true }
            | Self::ValueVertex { srgb: true, .. }
            | Self::TextureVertex { srgb: true, .. } => {
                MaterialFlags::ALBEDO_ACTIVE | MaterialFlags::ALBEDO_BLEND | MaterialFlags::ALBEDO_VERTEX_SRGB
            }
        }
    }

    pub(crate) fn is_texture(&self) -> bool {
        matches!(*self, Self::Texture(..) | Self::TextureVertex { .. })
    }

    pub(crate) fn to_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::None | Self::Vertex { .. } | Self::Value(_) | Self::ValueVertex { .. } => None,
            Self::Texture(handle) | Self::TextureVertex { handle, .. } | Self::TextureValue { handle, .. } => {
                Some(func(handle))
            }
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

    pub(crate) fn is_texture(&self) -> bool {
        matches!(*self, Self::Texture(..))
    }

    pub(crate) fn to_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::None | Self::Value(_) => None,
            Self::Texture(texture) => Some(func(texture)),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum NormalTexture {
    /// No normal texture
    None,
    /// Normal stored in RGB values
    Tricomponent(TextureHandle),
    /// Normal stored in RG values, third value should be reconstructed.
    Bicomponent(TextureHandle),
    /// Normal stored in Green and Alpha values, third value should be reconstructed.
    /// This is useful for storing in BC3 or BC7 compressed textures.
    BicomponentSwizzled(TextureHandle),
}
impl Default for NormalTexture {
    fn default() -> Self {
        Self::None
    }
}

impl NormalTexture {
    pub(crate) fn to_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::None => None,
            Self::Tricomponent(handle) | Self::Bicomponent(handle) | Self::BicomponentSwizzled(handle) => {
                Some(func(handle))
            }
        }
    }

    pub(crate) fn to_flags(&self) -> MaterialFlags {
        match self {
            Self::None => MaterialFlags::empty(),
            Self::Tricomponent(..) => MaterialFlags::empty(),
            Self::Bicomponent(..) => MaterialFlags::BICOMPONENT_NORMAL,
            Self::BicomponentSwizzled(..) => MaterialFlags::BICOMPONENT_NORMAL | MaterialFlags::SWIZZLED_NORMAL,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum AoMRTextures {
    None,
    GltfCombined {
        /// Texture with Ambient Occlusion in R, Metallic in G, and Roughness in B
        texture: Option<TextureHandle>,
    },
    GltfSplit {
        /// Texture with Ambient Occlusion in R
        ao_texture: Option<TextureHandle>,
        /// Texture with Metallic in G, and Roughness in B
        mr_texture: Option<TextureHandle>,
    },
    BWSplit {
        /// Texture with Ambient Occlusion in R
        ao_texture: Option<TextureHandle>,
        /// Texture with Metallic in R
        m_texture: Option<TextureHandle>,
        /// Texture with Roughness in R
        r_texture: Option<TextureHandle>,
    },
}

impl AoMRTextures {
    pub(crate) fn to_roughness_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined { texture: Some(texture) } => Some(func(texture)),
            Self::GltfSplit {
                mr_texture: Some(texture),
                ..
            } => Some(func(texture)),
            Self::BWSplit {
                r_texture: Some(texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub(crate) fn to_metallic_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined { .. } => None,
            Self::GltfSplit { .. } => None,
            Self::BWSplit {
                m_texture: Some(texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub(crate) fn to_ao_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined { .. } => None,
            Self::GltfSplit {
                ao_texture: Some(texture),
                ..
            } => Some(func(texture)),
            Self::BWSplit {
                ao_texture: Some(texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub(crate) fn to_flags(&self) -> MaterialFlags {
        match self {
            Self::GltfCombined { .. } => MaterialFlags::AOMR_GLTF_COMBINED,
            Self::GltfSplit { .. } => MaterialFlags::AOMR_GLTF_SPLIT,
            Self::BWSplit { .. } => MaterialFlags::AOMR_BW_SPLIT,
            // Use AOMR_GLTF_COMBINED so shader only checks roughness texture, then bails
            Self::None => MaterialFlags::AOMR_GLTF_COMBINED,
        }
    }
}
impl Default for AoMRTextures {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ClearcoatTextures {
    GltfCombined {
        /// Texture with Clearcoat in R, and Clearcoat Roughness in G
        texture: Option<TextureHandle>,
    },
    GltfSplit {
        /// Texture with Clearcoat in R
        clearcoat_texture: Option<TextureHandle>,
        /// Texture with Clearcoat Roughness in G
        clearcoat_roughness_texture: Option<TextureHandle>,
    },
    BWSplit {
        /// Texture with Clearcoat in R
        clearcoat_texture: Option<TextureHandle>,
        /// Texture with Clearcoat Roughness in R
        clearcoat_roughness_texture: Option<TextureHandle>,
    },
    None,
}

impl ClearcoatTextures {
    pub(crate) fn to_clearcoat_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined { texture: Some(texture) } => Some(func(texture)),
            Self::GltfSplit {
                clearcoat_texture: Some(texture),
                ..
            } => Some(func(texture)),
            Self::BWSplit {
                clearcoat_texture: Some(texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub(crate) fn to_clearcoat_roughness_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined { .. } => None,
            Self::GltfSplit {
                clearcoat_roughness_texture: Some(texture),
                ..
            } => Some(func(texture)),
            Self::BWSplit {
                clearcoat_roughness_texture: Some(texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub(crate) fn to_flags(&self) -> MaterialFlags {
        match self {
            Self::GltfCombined { .. } => MaterialFlags::CC_GLTF_COMBINED,
            Self::GltfSplit { .. } => MaterialFlags::CC_GLTF_SPLIT,
            Self::BWSplit { .. } => MaterialFlags::CC_BW_SPLIT,
            // Use CC_GLTF_COMBINED so shader only checks clear coat texture, then bails
            Self::None => MaterialFlags::CC_GLTF_COMBINED,
        }
    }
}
impl Default for ClearcoatTextures {
    fn default() -> Self {
        Self::None
    }
}

// Consider:
//
// - Green screen value
changeable_struct! {
    #[derive(Debug, Default, Copy, Clone)]
    pub struct Material <- nodefault MaterialChange {
        pub albedo: AlbedoComponent,
        pub normal: NormalTexture,
        pub aomr_textures: AoMRTextures,
        pub ao_factor: Option<f32>,
        pub metallic_factor: Option<f32>,
        pub roughness_factor: Option<f32>,
        pub clearcoat_textures: ClearcoatTextures,
        pub clearcoat_factor: Option<f32>,
        pub clearcoat_roughness_factor: Option<f32>,
        pub reflectance: MaterialComponent<f32>,
        pub anisotropy: MaterialComponent<f32>,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PipelineInputType {
    FullscreenTriangle,
    Models3d,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum PipelineBindingType {
    GeneralData,
    ObjectData,
    GPUMaterial,
    CPUMaterial,
    CameraData,
    GPU2DTextures,
    GPUCubeTextures,
    ShadowTexture,
    SkyboxTexture,
    Custom2DTexture { count: usize },
    CustomCubeTexture { count: usize },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DepthCompare {
    Closer,
    CloserEqual,
    Equal,
    Further,
    FurtherEqual,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PipelineOutputAttachment {
    pub format: ImageFormat,
    pub write: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PipelineDepthState {
    pub format: ImageFormat,
    pub compare: DepthCompare,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    // TODO: Alpha
    pub run_rate: RenderPassRunRate,
    pub input: PipelineInputType,
    pub outputs: Vec<PipelineOutputAttachment>,
    pub depth: Option<PipelineDepthState>,
    pub vertex: ShaderHandle,
    pub fragment: Option<ShaderHandle>,
    pub bindings: Vec<PipelineBindingType>,
    pub samples: u8,
}
