//! Types which make up `rend3-routine`'s material [`PbrMaterial`]

use encase::ShaderType;
use glam::{Mat3, Vec3, Vec4};
use rend3::types::{
    Material, RawTexture2DHandle, Sorting, Texture2DHandle, VertexAttributeId, VERTEX_ATTRIBUTE_COLOR_0,
    VERTEX_ATTRIBUTE_NORMAL, VERTEX_ATTRIBUTE_POSITION, VERTEX_ATTRIBUTE_TANGENT,
    VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_0, VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_1,
};

bitflags::bitflags! {
    /// Flags which shaders use to determine properties of a material
    #[derive(Default)]
    pub struct MaterialFlags : u32 {
        const ALBEDO_ACTIVE =       0b0000_0000_0000_0001;
        const ALBEDO_BLEND =        0b0000_0000_0000_0010;
        const ALBEDO_VERTEX_SRGB =  0b0000_0000_0000_0100;
        const BICOMPONENT_NORMAL =  0b0000_0000_0000_1000;
        const SWIZZLED_NORMAL =     0b0000_0000_0001_0000;
        const YDOWN_NORMAL =        0b0000_0000_0010_0000;
        const AOMR_COMBINED =       0b0000_0000_0100_0000;
        const AOMR_SWIZZLED_SPLIT = 0b0000_0000_1000_0000;
        const AOMR_SPLIT =          0b0000_0001_0000_0000;
        const AOMR_BW_SPLIT =       0b0000_0010_0000_0000;
        const CC_GLTF_COMBINED =    0b0000_0100_0000_0000;
        const CC_GLTF_SPLIT =       0b0000_1000_0000_0000;
        const CC_BW_SPLIT =         0b0001_0000_0000_0000;
        const UNLIT =               0b0010_0000_0000_0000;
        const NEAREST =             0b0100_0000_0000_0000;
    }
}

/// How the albedo color should be determined.
#[derive(Debug, Clone)]
pub enum AlbedoComponent {
    /// No albedo color.
    None,
    /// Albedo color is the vertex value.
    Vertex {
        /// Vertex should be converted from srgb -> linear before
        /// multiplication.
        srgb: bool,
    },
    /// Albedo color is the given value.
    Value(Vec4),
    /// Albedo color is the given value multiplied by the vertex color.
    ValueVertex {
        value: Vec4,
        /// Vertex should be converted from srgb -> linear before
        /// multiplication.
        srgb: bool,
    },
    /// Albedo color is loaded from the given texture.
    Texture(Texture2DHandle),
    /// Albedo color is loaded from the given texture, then multiplied
    /// by the vertex color.
    TextureVertex {
        texture: Texture2DHandle,
        /// Vertex should be converted from srgb -> linear before
        /// multiplication.
        srgb: bool,
    },
    /// Albedo color is loaded from given texture, then multiplied
    /// by the given value.
    TextureValue { texture: Texture2DHandle, value: Vec4 },
    /// Albedo color is loaded from the given texture, then multiplied
    /// by the vertex color and the given value.
    TextureVertexValue {
        texture: Texture2DHandle,
        /// Vertex should be converted from srgb -> linear before
        /// multiplication.
        srgb: bool,
        value: Vec4,
    },
}

impl Default for AlbedoComponent {
    fn default() -> Self {
        Self::None
    }
}

impl AlbedoComponent {
    pub fn to_value(&self) -> Vec4 {
        match *self {
            Self::Value(value) => value,
            Self::ValueVertex { value, .. } => value,
            Self::TextureValue { value, .. } => value,
            _ => Vec4::splat(1.0),
        }
    }

    pub fn to_flags(&self) -> MaterialFlags {
        match *self {
            Self::None => MaterialFlags::empty(),
            Self::Value(_) | Self::Texture(_) | Self::TextureValue { .. } => MaterialFlags::ALBEDO_ACTIVE,
            Self::Vertex { srgb: false }
            | Self::ValueVertex { srgb: false, .. }
            | Self::TextureVertex { srgb: false, .. }
            | Self::TextureVertexValue { srgb: false, .. } => {
                MaterialFlags::ALBEDO_ACTIVE | MaterialFlags::ALBEDO_BLEND
            }
            Self::Vertex { srgb: true }
            | Self::ValueVertex { srgb: true, .. }
            | Self::TextureVertex { srgb: true, .. }
            | Self::TextureVertexValue { srgb: true, .. } => {
                MaterialFlags::ALBEDO_ACTIVE | MaterialFlags::ALBEDO_BLEND | MaterialFlags::ALBEDO_VERTEX_SRGB
            }
        }
    }

    pub fn is_texture(&self) -> bool {
        matches!(
            *self,
            Self::Texture(..)
                | Self::TextureVertex { .. }
                | Self::TextureValue { .. }
                | Self::TextureVertexValue { .. }
        )
    }

    pub fn to_texture(&self) -> Option<&Texture2DHandle> {
        match *self {
            Self::None | Self::Vertex { .. } | Self::Value(_) | Self::ValueVertex { .. } => None,
            Self::Texture(ref texture)
            | Self::TextureVertex { ref texture, .. }
            | Self::TextureValue { ref texture, .. }
            | Self::TextureVertexValue { ref texture, .. } => Some(texture),
        }
    }
}

/// Generic container for a component of a material that could either be from a
/// texture or a fixed value.
#[derive(Debug, Clone)]
pub enum MaterialComponent<T> {
    None,
    Value(T),
    Texture(Texture2DHandle),
    TextureValue { texture: Texture2DHandle, value: T },
}

impl<T> Default for MaterialComponent<T> {
    fn default() -> Self {
        Self::None
    }
}

impl<T: Copy> MaterialComponent<T> {
    pub fn to_value(&self, default: T) -> T {
        match *self {
            Self::Value(value) | Self::TextureValue { value, .. } => value,
            Self::None | Self::Texture(_) => default,
        }
    }

    pub fn is_texture(&self) -> bool {
        matches!(*self, Self::Texture(..) | Self::TextureValue { .. })
    }

    pub fn to_texture(&self) -> Option<&Texture2DHandle> {
        match *self {
            Self::None | Self::Value(_) => None,
            Self::Texture(ref texture) | Self::TextureValue { ref texture, .. } => Some(texture),
        }
    }
}

/// The direction of the Y (i.e. green) value in the normal maps
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum NormalTextureYDirection {
    /// Right handed. X right, Y up. OpenGL convention.
    Up,
    /// Left handed. X right, Y down. DirectX convention.
    Down,
}

impl Default for NormalTextureYDirection {
    fn default() -> Self {
        Self::Up
    }
}

/// How normals should be derived
#[derive(Debug, Clone)]
pub enum NormalTexture {
    /// No normal texture.
    None,
    /// Normal stored in RGB values.
    Tricomponent(Texture2DHandle, NormalTextureYDirection),
    /// Normal stored in RG values, third value should be reconstructed.
    Bicomponent(Texture2DHandle, NormalTextureYDirection),
    /// Normal stored in Green and Alpha values, third value should be
    /// reconstructed. This is useful for storing in BC3 or BC7 compressed
    /// textures.
    BicomponentSwizzled(Texture2DHandle, NormalTextureYDirection),
}
impl Default for NormalTexture {
    fn default() -> Self {
        Self::None
    }
}

impl NormalTexture {
    pub fn to_texture(&self) -> Option<&Texture2DHandle> {
        match *self {
            Self::None => None,
            Self::Tricomponent(ref texture, _)
            | Self::Bicomponent(ref texture, _)
            | Self::BicomponentSwizzled(ref texture, _) => Some(texture),
        }
    }

    pub fn to_flags(&self) -> MaterialFlags {
        // Start with the base component flags
        let base = match self {
            Self::None => MaterialFlags::empty(),
            Self::Tricomponent(..) => MaterialFlags::empty(),
            Self::Bicomponent(..) => MaterialFlags::BICOMPONENT_NORMAL,
            Self::BicomponentSwizzled(..) => MaterialFlags::BICOMPONENT_NORMAL | MaterialFlags::SWIZZLED_NORMAL,
        };

        // Add the direction flags
        match self {
            Self::Tricomponent(_, NormalTextureYDirection::Down)
            | Self::Bicomponent(_, NormalTextureYDirection::Down)
            | Self::BicomponentSwizzled(_, NormalTextureYDirection::Down) => base | MaterialFlags::YDOWN_NORMAL,
            _ => base,
        }
    }
}

/// How the Ambient Occlusion, Metalic, and Roughness values should be
/// determined.
#[derive(Debug, Clone)]
pub enum AoMRTextures {
    None,
    Combined {
        /// Texture with Ambient Occlusion in R, Roughness in G, and Metallic in
        /// B
        texture: Option<Texture2DHandle>,
    },
    SwizzledSplit {
        /// Texture with Ambient Occlusion in R
        ao_texture: Option<Texture2DHandle>,
        /// Texture with Roughness in G and Metallic in B
        mr_texture: Option<Texture2DHandle>,
    },
    Split {
        /// Texture with Ambient Occlusion in R
        ao_texture: Option<Texture2DHandle>,
        /// Texture with Roughness in R and Metallic in G
        mr_texture: Option<Texture2DHandle>,
    },
    BWSplit {
        /// Texture with Ambient Occlusion in R
        ao_texture: Option<Texture2DHandle>,
        /// Texture with Metallic in R
        m_texture: Option<Texture2DHandle>,
        /// Texture with Roughness in R
        r_texture: Option<Texture2DHandle>,
    },
}

impl AoMRTextures {
    pub fn to_roughness_texture(&self) -> Option<&Texture2DHandle> {
        match *self {
            Self::Combined { texture: Some(ref texture) } => Some(texture),
            Self::SwizzledSplit { mr_texture: Some(ref texture), .. } => Some(texture),
            Self::Split { mr_texture: Some(ref texture), .. } => Some(texture),
            Self::BWSplit { r_texture: Some(ref texture), .. } => Some(texture),
            _ => None,
        }
    }

    pub fn to_metallic_texture(&self) -> Option<&Texture2DHandle> {
        match *self {
            Self::Combined { .. } => None,
            Self::SwizzledSplit { .. } => None,
            Self::Split { .. } => None,
            Self::BWSplit { m_texture: Some(ref texture), .. } => Some(texture),
            _ => None,
        }
    }

    pub fn to_ao_texture(&self) -> Option<&Texture2DHandle> {
        match *self {
            Self::Combined { .. } => None,
            Self::SwizzledSplit { ao_texture: Some(ref texture), .. } => Some(texture),
            Self::Split { ao_texture: Some(ref texture), .. } => Some(texture),
            Self::BWSplit { ao_texture: Some(ref texture), .. } => Some(texture),
            _ => None,
        }
    }

    pub fn to_flags(&self) -> MaterialFlags {
        match self {
            Self::Combined { .. } => MaterialFlags::AOMR_COMBINED,
            Self::SwizzledSplit { .. } => MaterialFlags::AOMR_SWIZZLED_SPLIT,
            Self::Split { .. } => MaterialFlags::AOMR_SPLIT,
            Self::BWSplit { .. } => MaterialFlags::AOMR_BW_SPLIT,
            // Use AOMR_COMBINED so shader only checks roughness texture, then bails
            Self::None => MaterialFlags::AOMR_COMBINED,
        }
    }
}
impl Default for AoMRTextures {
    fn default() -> Self {
        Self::None
    }
}

/// How clearcoat values should be derived.
#[derive(Debug, Clone)]
pub enum ClearcoatTextures {
    GltfCombined {
        /// Texture with Clearcoat in R, and Clearcoat Roughness in G
        texture: Option<Texture2DHandle>,
    },
    GltfSplit {
        /// Texture with Clearcoat in R
        clearcoat_texture: Option<Texture2DHandle>,
        /// Texture with Clearcoat Roughness in G
        clearcoat_roughness_texture: Option<Texture2DHandle>,
    },
    BWSplit {
        /// Texture with Clearcoat in R
        clearcoat_texture: Option<Texture2DHandle>,
        /// Texture with Clearcoat Roughness in R
        clearcoat_roughness_texture: Option<Texture2DHandle>,
    },
    None,
}

impl ClearcoatTextures {
    pub fn to_clearcoat_texture(&self) -> Option<&Texture2DHandle> {
        match *self {
            Self::GltfCombined { texture: Some(ref texture) } => Some(texture),
            Self::GltfSplit { clearcoat_texture: Some(ref texture), .. } => Some(texture),
            Self::BWSplit { clearcoat_texture: Some(ref texture), .. } => Some(texture),
            _ => None,
        }
    }

    pub fn to_clearcoat_roughness_texture(&self) -> Option<&Texture2DHandle> {
        match *self {
            Self::GltfCombined { .. } => None,
            Self::GltfSplit { clearcoat_roughness_texture: Some(ref texture), .. } => Some(texture),
            Self::BWSplit { clearcoat_roughness_texture: Some(ref texture), .. } => Some(texture),
            _ => None,
        }
    }

    pub fn to_flags(&self) -> MaterialFlags {
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

/// How textures should be sampled.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SampleType {
    Nearest,
    Linear,
}
impl Default for SampleType {
    fn default() -> Self {
        Self::Linear
    }
}

/// The type of transparency in a material.
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TransparencyType {
    /// Alpha is completely ignored.
    Opaque,
    /// Alpha less than a specified value is discorded.
    Cutout,
    /// Alpha is blended.
    Blend,
}
impl From<Transparency> for TransparencyType {
    fn from(t: Transparency) -> Self {
        match t {
            Transparency::Opaque => Self::Opaque,
            Transparency::Cutout { .. } => Self::Cutout,
            Transparency::Blend => Self::Blend,
        }
    }
}
impl TransparencyType {
    pub fn to_debug_str(self) -> &'static str {
        match self {
            TransparencyType::Opaque => "opaque",
            TransparencyType::Cutout => "cutout",
            TransparencyType::Blend => "blend",
        }
    }

    pub fn to_sorting(self) -> Sorting {
        match self {
            Self::Opaque | Self::Cutout => Sorting::OPAQUE,
            Self::Blend => Sorting::BLENDING,
        }
    }
}

#[allow(clippy::cmp_owned)] // This thinks making a temporary TransparencyType is the end of the world
impl PartialEq<Transparency> for TransparencyType {
    fn eq(&self, other: &Transparency) -> bool {
        *self == Self::from(*other)
    }
}

#[allow(clippy::cmp_owned)]
impl PartialEq<TransparencyType> for Transparency {
    fn eq(&self, other: &TransparencyType) -> bool {
        TransparencyType::from(*self) == *other
    }
}

/// How transparency should be handled in a material.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Transparency {
    /// Alpha is completely ignored.
    Opaque,
    /// Pixels with alpha less than `cutout` is discorded.
    Cutout { cutout: f32 },
    /// Alpha is blended.
    Blend,
}
impl Default for Transparency {
    fn default() -> Self {
        Self::Opaque
    }
}

// Consider:
//
// - Green screen value
/// A set of textures and values that determine the how an object interacts with
/// light.
#[derive(Default)]
pub struct PbrMaterial {
    pub albedo: AlbedoComponent,
    pub transparency: Transparency,
    pub normal: NormalTexture,
    pub aomr_textures: AoMRTextures,
    pub ao_factor: Option<f32>,
    pub metallic_factor: Option<f32>,
    pub roughness_factor: Option<f32>,
    pub clearcoat_textures: ClearcoatTextures,
    pub clearcoat_factor: Option<f32>,
    pub clearcoat_roughness_factor: Option<f32>,
    pub emissive: MaterialComponent<Vec3>,
    pub reflectance: MaterialComponent<f32>,
    pub anisotropy: MaterialComponent<f32>,
    pub uv_transform0: Mat3,
    pub uv_transform1: Mat3,
    // TODO: Make unlit a different shader entirely.
    pub unlit: bool,
    pub sample_type: SampleType,
}

impl Material for PbrMaterial {
    type DataType = ShaderMaterial;
    type TextureArrayType = [Option<RawTexture2DHandle>; 10];
    type RequiredAttributeArrayType = [&'static VertexAttributeId; 1];
    type SupportedAttributeArrayType = [&'static VertexAttributeId; 6];

    fn required_attributes() -> Self::RequiredAttributeArrayType {
        [&VERTEX_ATTRIBUTE_POSITION]
    }

    fn supported_attributes() -> Self::SupportedAttributeArrayType {
        [
            &VERTEX_ATTRIBUTE_POSITION,
            &VERTEX_ATTRIBUTE_NORMAL,
            &VERTEX_ATTRIBUTE_TANGENT,
            &VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_0,
            &VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_1,
            &VERTEX_ATTRIBUTE_COLOR_0,
        ]
    }

    fn key(&self) -> u64 {
        TransparencyType::from(self.transparency) as u64
    }

    fn sorting(&self) -> Sorting {
        TransparencyType::from(self.transparency).to_sorting()
    }

    fn to_textures(&self) -> Self::TextureArrayType {
        [
            self.albedo.to_texture(),
            self.normal.to_texture(),
            self.aomr_textures.to_roughness_texture(),
            self.aomr_textures.to_metallic_texture(),
            self.reflectance.to_texture(),
            self.clearcoat_textures.to_clearcoat_texture(),
            self.clearcoat_textures.to_clearcoat_roughness_texture(),
            self.emissive.to_texture(),
            self.anisotropy.to_texture(),
            self.aomr_textures.to_ao_texture(),
        ]
        .map(|opt| opt.map(|r| *r.get_raw()))
    }

    fn to_data(&self) -> Self::DataType {
        ShaderMaterial::from_material(self)
    }
}

#[derive(Debug, Default, Copy, Clone, ShaderType)]
pub struct ShaderMaterial {
    uv_transform0: Mat3,
    uv_transform1: Mat3,

    albedo: Vec4,
    emissive: Vec3,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    anisotropy: f32,
    ambient_occlusion: f32,
    alpha_cutout: f32,

    material_flags: u32,
}

unsafe impl bytemuck::Zeroable for ShaderMaterial {}
unsafe impl bytemuck::Pod for ShaderMaterial {}

impl ShaderMaterial {
    fn from_material(material: &PbrMaterial) -> Self {
        Self {
            uv_transform0: material.uv_transform0,
            uv_transform1: material.uv_transform1,
            albedo: material.albedo.to_value(),
            roughness: material.roughness_factor.unwrap_or(0.0),
            metallic: material.metallic_factor.unwrap_or(0.0),
            reflectance: material.reflectance.to_value(0.5),
            clear_coat: material.clearcoat_factor.unwrap_or(0.0),
            clear_coat_roughness: material.clearcoat_roughness_factor.unwrap_or(0.0),
            emissive: material.emissive.to_value(Vec3::ZERO),
            anisotropy: material.anisotropy.to_value(0.0),
            ambient_occlusion: material.ao_factor.unwrap_or(1.0),
            alpha_cutout: match material.transparency {
                Transparency::Cutout { cutout } => cutout,
                _ => 0.0,
            },
            material_flags: {
                let mut flags = material.albedo.to_flags();
                flags |= material.normal.to_flags();
                flags |= material.aomr_textures.to_flags();
                flags |= material.clearcoat_textures.to_flags();
                flags.set(MaterialFlags::UNLIT, material.unlit);
                flags.set(
                    MaterialFlags::NEAREST,
                    match material.sample_type {
                        SampleType::Nearest => true,
                        SampleType::Linear => false,
                    },
                );
                flags.bits()
            },
        }
    }
}
