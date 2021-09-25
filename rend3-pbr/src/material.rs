bitflags::bitflags! {
    /// Flags which shaders use to determine properties of a material
    pub struct MaterialFlags : u32 {
        const ALBEDO_ACTIVE =      0b0000_0000_0000_0001;
        const ALBEDO_BLEND =       0b0000_0000_0000_0010;
        const ALBEDO_VERTEX_SRGB = 0b0000_0000_0000_0100;
        /// TODO hole
        const BICOMPONENT_NORMAL = 0b0000_0000_0001_0000;
        const SWIZZLED_NORMAL =    0b0000_0000_0010_0000;
        const AOMR_GLTF_COMBINED = 0b0000_0000_0100_0000;
        const AOMR_GLTF_SPLIT =    0b0000_0000_1000_0000;
        const AOMR_BW_SPLIT =      0b0000_0001_0000_0000;
        const CC_GLTF_COMBINED =   0b0000_0010_0000_0000;
        const CC_GLTF_SPLIT =      0b0000_0100_0000_0000;
        const CC_BW_SPLIT =        0b0000_1000_0000_0000;
        const UNLIT =              0b0001_0000_0000_0000;
        const NEAREST =            0b0010_0000_0000_0000;
    }
}

/// How the albedo color should be determined.
#[derive(Debug, Clone)]
pub enum AlbedoComponent {
    /// No albedo color.
    None,
    /// Albedo color is the vertex value.
    Vertex {
        /// Vertex should be converted from srgb -> linear before multiplication.
        srgb: bool,
    },
    /// Albedo color is the given value.
    Value(Vec4),
    /// Albedo color is the given value multiplied by the vertex color.
    ValueVertex {
        value: Vec4,
        /// Vertex should be converted from srgb -> linear before multiplication.
        srgb: bool,
    },
    /// Albedo color is loaded from the given texture.
    Texture(TextureHandle),
    /// Albedo color is loaded from the given texture, then multiplied
    /// by the vertex color.
    TextureVertex {
        texture: TextureHandle,
        /// Vertex should be converted from srgb -> linear before multiplication.
        srgb: bool,
    },
    /// Albedo color is loaded from given texture, then multiplied
    /// by the given value.
    TextureValue { texture: TextureHandle, value: Vec4 },
    /// Albedo color is loaded from the given texture, then multiplied
    /// by the vertex color and the given value.
    TextureVertexValue {
        texture: TextureHandle,
        /// Vertex should be converted from srgb -> linear before multiplication.
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

    pub fn to_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(&TextureHandle) -> Out,
    {
        match *self {
            Self::None | Self::Vertex { .. } | Self::Value(_) | Self::ValueVertex { .. } => None,
            Self::Texture(ref texture)
            | Self::TextureVertex { ref texture, .. }
            | Self::TextureValue { ref texture, .. }
            | Self::TextureVertexValue { ref texture, .. } => Some(func(texture)),
        }
    }
}

/// Generic container for a component of a material that could either be from a texture or a fixed value.
#[derive(Debug, Clone)]
pub enum MaterialComponent<T> {
    None,
    Value(T),
    Texture(TextureHandle),
    TextureValue { texture: TextureHandle, value: T },
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

    pub fn to_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(&TextureHandle) -> Out,
    {
        match *self {
            Self::None | Self::Value(_) => None,
            Self::Texture(ref texture) | Self::TextureValue { ref texture, .. } => Some(func(texture)),
        }
    }
}

/// How normals should be derived
#[derive(Debug, Clone)]
pub enum NormalTexture {
    /// No normal texture.
    None,
    /// Normal stored in RGB values.
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
    pub fn to_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(&TextureHandle) -> Out,
    {
        match *self {
            Self::None => None,
            Self::Tricomponent(ref texture)
            | Self::Bicomponent(ref texture)
            | Self::BicomponentSwizzled(ref texture) => Some(func(texture)),
        }
    }

    pub fn to_flags(&self) -> MaterialFlags {
        match self {
            Self::None => MaterialFlags::empty(),
            Self::Tricomponent(..) => MaterialFlags::empty(),
            Self::Bicomponent(..) => MaterialFlags::BICOMPONENT_NORMAL,
            Self::BicomponentSwizzled(..) => MaterialFlags::BICOMPONENT_NORMAL | MaterialFlags::SWIZZLED_NORMAL,
        }
    }
}

/// How the Ambient Occlusion, Metalic, and Roughness values should be determined.
#[derive(Debug, Clone)]
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
    pub fn to_roughness_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(&TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined {
                texture: Some(ref texture),
            } => Some(func(texture)),
            Self::GltfSplit {
                mr_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
            Self::BWSplit {
                r_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub fn to_metallic_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(&TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined { .. } => None,
            Self::GltfSplit { .. } => None,
            Self::BWSplit {
                m_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub fn to_ao_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(&TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined { .. } => None,
            Self::GltfSplit {
                ao_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
            Self::BWSplit {
                ao_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub fn to_flags(&self) -> MaterialFlags {
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

/// How clearcoat values should be derived.
#[derive(Debug, Clone)]
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
    pub fn to_clearcoat_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(&TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined {
                texture: Some(ref texture),
            } => Some(func(texture)),
            Self::GltfSplit {
                clearcoat_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
            Self::BWSplit {
                clearcoat_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
            _ => None,
        }
    }

    pub fn to_clearcoat_roughness_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(&TextureHandle) -> Out,
    {
        match *self {
            Self::GltfCombined { .. } => None,
            Self::GltfSplit {
                clearcoat_roughness_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
            Self::BWSplit {
                clearcoat_roughness_texture: Some(ref texture),
                ..
            } => Some(func(texture)),
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
#[derive(Debug, Copy, Clone, PartialEq)]
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
changeable_struct! {
    /// A set of textures and values that determine the how an object interacts with light.
    pub struct Material <- nodefault MaterialChange {
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
        // TODO: Determine how to make this a clearer part of the type system, esp. with the changable_struct macro.
        pub unlit: bool,
        pub sample_type: SampleType,
    }
}