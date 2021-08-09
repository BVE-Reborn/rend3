use glam::{Mat3, Mat4, Vec2, Vec3, Vec3A, Vec4};
use std::mem;
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
    DirectionalLightHandle
);

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
pub struct InterleavedModelVertex {
    pub position: Vec3,      // 00..12
    pub normal: Vec3,        // 12..24
    pub uv: Vec2,            // 24..32
    pub color: [u8; 4],      // 32..36
    pub material_index: u32, // 36..40
}

/// Easy to use builder for a [`Mesh`] that deals with common operations for you.
#[derive(Debug, Default)]
pub struct MeshBuilder {
    vertex_positions: Vec<Vec3>,
    vertex_normals: Option<Vec<Vec3>>,
    vertex_tangents: Option<Vec<Vec3>>,
    vertex_uvs: Option<Vec<Vec2>>,
    vertex_colors: Option<Vec<[u8; 4]>>,
    vertex_material_indices: Option<Vec<u32>>,
    vertex_count: usize,

    indices: Option<Vec<u32>>,

    right_handed: bool,
}
impl MeshBuilder {
    /// Create a new [`MeshBuilder`] with a given set of positions.
    ///
    /// All vertices must have positions.
    ///
    /// # Panic
    ///
    /// Will panic if the length is zero.
    pub fn new(vertex_positions: Vec<Vec3>) -> Self {
        let me = Self {
            vertex_count: vertex_positions.len(),
            vertex_positions,
            ..Self::default()
        };
        assert_ne!(me.vertex_positions.len(), 0, "Cannot have a mesh with zero vertices");
        me
    }

    fn validate_len(&self, len: usize) {
        assert_eq!(self.vertex_count, len)
    }

    /// Add vertex normals to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_normals(mut self, normals: Vec<Vec3>) -> Self {
        self.validate_len(normals.len());
        self.vertex_normals = Some(normals);
        self
    }

    /// Add vertex tangents to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_tangents(mut self, tangents: Vec<Vec3>) -> Self {
        self.validate_len(tangents.len());
        self.vertex_tangents = Some(tangents);
        self
    }

    /// Add texture coordinates to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_uvs(mut self, uvs: Vec<Vec2>) -> Self {
        self.validate_len(uvs.len());
        self.vertex_uvs = Some(uvs);
        self
    }

    /// Add vertex colors to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_colors(mut self, colors: Vec<[u8; 4]>) -> Self {
        self.validate_len(colors.len());
        self.vertex_colors = Some(colors);
        self
    }

    /// Add material indices to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_material_indices(mut self, material_indices: Vec<u32>) -> Self {
        self.validate_len(material_indices.len());
        self.vertex_material_indices = Some(material_indices);
        self
    }

    /// Add indices to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is zero.
    pub fn with_indices(mut self, indices: Vec<u32>) -> Self {
        assert_ne!(indices.len(), 0, "Cannot have a mesh with zero indices");
        self.indices = Some(indices);
        self
    }

    /// Mark this mesh as using a right handed (Counter Clockwise) winding order. It will be
    /// converted to rend3 native left handed (Clockwise) winding order on construction. This will
    /// not change the vertex normals. If this is called, it is advised to not provide a normal
    /// buffer so a buffer will be calculated for you.
    ///
    /// See [`Mesh::flip_winding_order`] for more information.
    pub fn with_right_handed(mut self) -> Self {
        self.right_handed = true;
        self
    }

    /// Build a mesh, adding whatever components weren't provided.
    ///
    /// If normals weren't provided, they will be calculated. If mesh
    /// is right handed, will be converted to left handed.
    ///
    /// All others will be filled with defaults.
    pub fn build(self) -> Mesh {
        let length = self.vertex_count;
        debug_assert_ne!(length, 0, "Length should be guarded by validation");

        let has_normals = self.vertex_normals.is_some();
        let has_tangents = self.vertex_tangents.is_some();

        let mut mesh = Mesh {
            vertex_positions: self.vertex_positions,
            vertex_normals: self.vertex_normals.unwrap_or_else(|| vec![Vec3::ZERO; length]),
            vertex_tangents: self.vertex_tangents.unwrap_or_else(|| vec![Vec3::ZERO; length]),
            vertex_uvs: self.vertex_uvs.unwrap_or_else(|| vec![Vec2::ZERO; length]),
            vertex_colors: self.vertex_colors.unwrap_or_else(|| vec![[0; 4]; length]),
            vertex_material_indices: self.vertex_material_indices.unwrap_or_else(|| vec![0; length]),
            indices: self.indices.unwrap_or_else(|| (0..length as u32).collect()),
        };

        // We need to flip winding order first, so the normals will be facing the right direction.
        if self.right_handed {
            mesh.flip_winding_order();
        }

        if !has_normals {
            mesh.calculate_normals();
        }

        if !has_tangents {
            mesh.calculate_tangents();
        }

        mesh
    }
}

/// Represents a mesh that may be used by many objects.
///
/// Meshes are in Structure of Array format and must have all the vertex_* arrays be the same length.
/// This condition can be checked with the [`Mesh::validate`] function.
///
/// These can be annoying to construct, so use the [`MeshBuilder`] to make it easier.
#[derive(Debug, Default, Clone)]
pub struct Mesh {
    pub vertex_positions: Vec<Vec3>,
    pub vertex_normals: Vec<Vec3>,
    pub vertex_tangents: Vec<Vec3>,
    pub vertex_uvs: Vec<Vec2>,
    pub vertex_colors: Vec<[u8; 4]>,
    pub vertex_material_indices: Vec<u32>,

    pub indices: Vec<u32>,
}

impl Mesh {
    /// Validates that all vertex attributes have the same length.
    pub fn validate(&self) -> bool {
        let position_lenth = self.vertex_positions.len();
        [
            self.vertex_normals.len(),
            self.vertex_tangents.len(),
            self.vertex_uvs.len(),
            self.vertex_colors.len(),
            self.vertex_material_indices.len(),
        ]
        .iter()
        .all(|v| *v == position_lenth)
    }

    /// Calculate normals for the given mesh, assuming smooth shading and per-vertex normals.
    ///
    /// Use left-handed normal calculation. Call [`Mesh::flip_winding_order`] first if you have
    /// a right handed mesh you want to use with rend3.
    pub fn calculate_normals(&mut self) {
        Self::calculate_normals_for_buffers(&mut self.vertex_normals, &self.vertex_positions, &self.indices);
    }

    /// Calculate normals for the given buffers representing a mesh, assuming smooth shading and per-vertex normals.
    ///
    /// Positions and normals must be the same length.
    pub fn calculate_normals_for_buffers(normals: &mut [Vec3], positions: &[Vec3], indices: &[u32]) {
        assert_eq!(normals.len(), positions.len());

        for norm in normals.iter_mut() {
            *norm = Vec3::ZERO;
        }

        for idx in indices.chunks_exact(3) {
            let (idx0, idx1, idx2) = match *idx {
                [idx0, idx1, idx2] => (idx0, idx1, idx2),
                // SAFETY: This is guaranteed by chunks_exact(3)
                _ => unsafe { std::hint::unreachable_unchecked() },
            };

            let pos1 = positions[idx0 as usize];
            let pos2 = positions[idx1 as usize];
            let pos3 = positions[idx2 as usize];

            let edge1 = pos2 - pos1;
            let edge2 = pos3 - pos1;

            let normal = edge1.cross(edge2);

            // SAFETY: All vectors are the same length by the assert, and indexing succeeded on positions, therefore it's safe on normals
            unsafe {
                *normals.get_unchecked_mut(idx0 as usize) += normal;
                *normals.get_unchecked_mut(idx1 as usize) += normal;
                *normals.get_unchecked_mut(idx2 as usize) += normal;
            }
        }

        for normal in normals.iter_mut() {
            *normal = normal.normalize();
        }
    }

    /// Calculate tangents for the given mesh, based on normals and texture coordinates
    pub fn calculate_tangents(&mut self) {
        Self::calculate_tangents_for_buffers(
            &mut self.vertex_tangents,
            &self.vertex_positions,
            &self.vertex_normals,
            &self.vertex_uvs,
            &self.indices,
        );
    }

    fn calculate_tangents_for_buffers(
        tangents: &mut [Vec3],
        positions: &[Vec3],
        normals: &[Vec3],
        uvs: &[Vec2],
        indices: &[u32],
    ) {
        assert_eq!(tangents.len(), positions.len());
        assert_eq!(uvs.len(), positions.len());

        for tan in tangents.iter_mut() {
            *tan = Vec3::ZERO;
        }

        for idx in indices.chunks_exact(3) {
            let (idx0, idx1, idx2) = match *idx {
                [idx0, idx1, idx2] => (idx0, idx1, idx2),
                // SAFETY: This is guaranteed by chunks_exact(3)
                _ => unsafe { std::hint::unreachable_unchecked() },
            };

            let pos1 = positions[idx0 as usize];
            let pos2 = positions[idx1 as usize];
            let pos3 = positions[idx2 as usize];

            // SAFETY: All vectors are the same length by the assert, and indexing succeeded on positions, therefore it's safe on uvs
            let (tex1, tex2, tex3) = unsafe {
                (
                    *uvs.get_unchecked(idx0 as usize),
                    *uvs.get_unchecked(idx1 as usize),
                    *uvs.get_unchecked(idx2 as usize),
                )
            };

            let edge1 = pos2 - pos1;
            let edge2 = pos3 - pos1;

            let uv1 = tex2 - tex1;
            let uv2 = tex3 - tex1;

            let r = 1.0 / (uv1.x * uv2.y - uv1.y * uv2.x);

            let tangent = Vec3::new(
                ((edge1.x * uv2.y) - (edge2.x * uv1.y)) * r,
                ((edge1.y * uv2.y) - (edge2.y * uv1.y)) * r,
                ((edge1.z * uv2.y) - (edge2.z * uv1.y)) * r,
            );

            // SAFETY: All vectors are the same length by the assert, and indexing succeeded on positions, therefore it's safe on tangents
            unsafe {
                *tangents.get_unchecked_mut(idx0 as usize) += tangent;
                *tangents.get_unchecked_mut(idx1 as usize) += tangent;
                *tangents.get_unchecked_mut(idx2 as usize) += tangent;
            }
        }

        for (tan, norm) in tangents.iter_mut().zip(normals) {
            let t = *tan - (*norm * norm.dot(*tan));
            *tan = t.normalize();
        }
    }

    /// Inverts the winding order of a mesh. This is useful if you have meshes which
    /// are designed for right-handed (Counter-Clockwise) winding order for use in OpenGL or VK.
    ///
    /// This does not change vertex location, so does not change coordinate system. This will
    /// also not change the vertex normals. Calling [`Mesh::calculate_normals`] is advised after
    /// calling this function.
    ///
    /// rend3 uses a left-handed (Clockwise) winding order.
    pub fn flip_winding_order(&mut self) {
        for indices in self.indices.chunks_exact_mut(3) {
            if let [left, _, right] = indices {
                mem::swap(left, right);
            } else {
                // SAFETY: chunks_exact(3) guarantees us 3 value long slices
                unsafe { std::hint::unreachable_unchecked() }
            }
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
        const ALBEDO_ACTIVE =      0b0000_0000_0000_0001;
        const ALBEDO_BLEND =       0b0000_0000_0000_0010;
        const ALBEDO_VERTEX_SRGB = 0b0000_0000_0000_0100;
        const ALPHA_CUTOUT =       0b0000_0000_0000_1000;
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
        matches!(
            *self,
            Self::Texture(..) | Self::TextureVertex { .. } | Self::TextureValue { .. }
        )
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
    TextureValue { handle: TextureHandle, value: T },
}

impl<T> Default for MaterialComponent<T> {
    fn default() -> Self {
        Self::None
    }
}

impl<T: Copy> MaterialComponent<T> {
    pub(crate) fn to_value(&self, default: T) -> T {
        match *self {
            Self::Value(value) | Self::TextureValue { value, .. } => value,
            Self::None | Self::Texture(_) => default,
        }
    }

    pub(crate) fn is_texture(&self) -> bool {
        matches!(*self, Self::Texture(..) | Self::TextureValue { .. })
    }

    pub(crate) fn to_texture<Func, Out>(&self, func: Func) -> Option<Out>
    where
        Func: FnOnce(TextureHandle) -> Out,
    {
        match *self {
            Self::None | Self::Value(_) => None,
            Self::Texture(handle) | Self::TextureValue { handle, .. } => Some(func(handle)),
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
        pub emissive: MaterialComponent<Vec3>,
        pub reflectance: MaterialComponent<f32>,
        pub anisotropy: MaterialComponent<f32>,
        pub alpha_cutout: Option<f32>,
        pub transform: Mat3,
        // TODO: Determine how to make this a clearer part of the type system, esp. with the changable_struct macro.
        pub unlit: bool,
        pub nearest: bool,
    }
}

#[derive(Debug, Clone)]
pub struct Object {
    pub mesh: MeshHandle,
    pub material: MaterialHandle,
    pub transform: Mat4,
}

#[derive(Debug, Default, Copy, Clone)]
pub struct Camera {
    pub projection: CameraProjection,
    pub location: Vec3A,
}

#[derive(Debug, Copy, Clone)]
pub enum CameraProjection {
    Orthographic {
        /// Size assumes the location is at the center of the camera area.
        size: Vec3A,
        direction: Vec3A,
    },
    Projection {
        /// Vertical field of view in degrees.
        vfov: f32,
        /// Near plane distance. All projection uses a infinite far plane.
        near: f32,
        /// Radians
        pitch: f32,
        /// Radians
        yaw: f32,
    },
}

impl CameraProjection {
    pub fn from_orthographic_direction(direction: Vec3A) -> Self {
        Self::Orthographic {
            size: Vec3A::new(100.0, 100.0, 200.0),
            direction,
        }
    }
}

impl Default for CameraProjection {
    fn default() -> Self {
        Self::Projection {
            vfov: 60.0,
            near: 0.1,
            pitch: 0.0,
            yaw: 0.0,
        }
    }
}

changeable_struct! {
    #[derive(Debug, Copy, Clone)]
    pub struct DirectionalLight <- DirectionalLightChange {
        pub color: Vec3,
        pub intensity: f32,
        pub direction: Vec3,
    }
}
