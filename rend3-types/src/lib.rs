//! Type declarations for the rend3 3D rendering crate.
//!
//! This is reexported in the rend3 crate proper and includes all the "surface"
//! api arguments.

use glam::{Mat4, UVec2, Vec2, Vec3, Vec3A, Vec4};
use std::{
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
    mem,
    num::NonZeroU32,
    sync::{Arc, Weak},
};
use thiserror::Error;

/// Reexport of the glam version rend3 is using.
pub use glam;

/// Non-owning resource handle.
///
/// Not part of rend3's external interface, but needed to interface with rend3's
/// internal datastructures if writing your own structures or render routines.
pub struct RawResourceHandle<T> {
    /// Underlying value of the handle.
    pub idx: usize,
    _phantom: PhantomData<T>,
}

// Need Debug/Copy/Clone impls that don't require T: Trait.
impl<T> Debug for RawResourceHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawResourceHandle").field("idx", &self.idx).finish()
    }
}

impl<T> Copy for RawResourceHandle<T> {}

impl<T> Clone for RawResourceHandle<T> {
    fn clone(&self) -> Self {
        Self {
            idx: self.idx,
            _phantom: PhantomData,
        }
    }
}

impl<T> PartialEq for RawResourceHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Eq for RawResourceHandle<T> {}

/// Owning resource handle. Used as part of rend3's interface.
pub struct ResourceHandle<T> {
    refcount: Arc<()>,
    idx: usize,
    _phantom: PhantomData<T>,
}

impl<T> Debug for ResourceHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceHandle")
            .field("refcount", &Arc::strong_count(&self.refcount))
            .field("idx", &self.idx)
            .finish()
    }
}

impl<T> Clone for ResourceHandle<T> {
    fn clone(&self) -> Self {
        Self {
            refcount: self.refcount.clone(),
            idx: self.idx,
            _phantom: self._phantom,
        }
    }
}

impl<T> PartialEq for ResourceHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Eq for ResourceHandle<T> {}

impl<T> Hash for ResourceHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
    }
}

impl<T> ResourceHandle<T> {
    /// Create a new resource handle from an index.
    ///
    /// Part of rend3's internal interface, use `Renderer::add_*` instead.
    pub fn new(idx: usize) -> Self {
        Self {
            refcount: Arc::new(()),
            idx,
            _phantom: PhantomData,
        }
    }

    /// Gets the equivalent raw handle for this owning handle.
    ///
    /// Part of rend3's internal interface for accessing internal resrouces
    pub fn get_raw(&self) -> RawResourceHandle<T> {
        RawResourceHandle {
            idx: self.idx,
            _phantom: PhantomData,
        }
    }

    /// Get the weak refcount for this owned handle.
    ///
    /// Part of rend3's internal interface.
    pub fn get_weak_refcount(&self) -> Weak<()> {
        Arc::downgrade(&self.refcount)
    }
}

#[macro_export]
#[doc(hidden)]
macro_rules! declare_handle {
    ($($name:ident<$ty:ty>),*) => {$(
        #[doc = concat!("Refcounted handle to a ", stringify!($ty) ,".")]
        pub type $name = ResourceHandle<$ty>;
    )*};
}

declare_handle!(
    MeshHandle<Mesh>,
    TextureHandle<Texture>,
    MaterialHandle<MaterialTag>,
    ObjectHandle<Object>,
    DirectionalLightHandle<DirectionalLight>,
    SkeletonHandle<Skeleton>
);

#[macro_export]
#[doc(hidden)]
macro_rules! declare_raw_handle {
    ($($name:ident<$ty:ty>),*) => {$(
        #[doc = concat!("Internal non-owning handle to a ", stringify!($ty) ,".")]
        pub type $name = RawResourceHandle<$ty>;
    )*};
}

declare_raw_handle!(
    RawMeshHandle<Mesh>,
    RawTextureHandle<Texture>,
    RawMaterialHandle<MaterialTag>,
    RawObjectHandle<Object>,
    RawDirectionalLightHandle<DirectionalLight>,
    RawSkeletonHandle<Skeleton>
);

macro_rules! changeable_struct {
    ($(#[$outer:meta])* pub struct $name:ident <- $name_change:ident { $($(#[$inner:meta])* $field_vis:vis $field_name:ident : $field_type:ty),* $(,)? } ) => {
        $(#[$outer])*
        #[derive(Debug, Clone)]
        pub struct $name {
            $(
                $(#[$inner])* $field_vis $field_name : $field_type
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
        #[doc = concat!("Describes a modification to a ", stringify!($name), ".")]
        #[derive(Debug, Default, Clone)]
        pub struct $name_change {
            $(
                $field_vis $field_name : Option<$field_type>
            ),*
        }
    };
}

// WGPU REEXPORTS
#[doc(inline)]
pub use wgt::{Backend, Backends, Color, DeviceType, PresentMode, TextureFormat, TextureUsages};

/// The maximum amount of vertices any one object can have.
///
/// The value allows for 8 bits of information packed in the high 8 bits of the
/// index for object recombination.
pub const MAX_VERTEX_COUNT: usize = 1 << 24;

/// Identifies the semantic use of a vertex buffer.
#[derive(Debug, Copy, Clone)]
pub enum VertexBufferType {
    Position,
    Normal,
    Tangent,
    Uv0,
    Uv1,
    Colors,
}

/// Error returned from mesh validation.
#[derive(Debug, Error)]
pub enum MeshValidationError {
    #[error("Mesh's {ty:?} buffer has {actual} vertices but the position buffer has {expected}")]
    MismatchedVertexCount {
        ty: VertexBufferType,
        expected: usize,
        actual: usize,
    },
    #[error("Mesh has {count} vertices when the vertex limit is {}", MAX_VERTEX_COUNT)]
    ExceededMaxVertexCount { count: usize },
    #[error("Mesh has {count} indices which is not a multiple of three. Meshes are always composed of triangles")]
    IndexCountNotMultipleOfThree { count: usize },
    #[error(
        "Index at position {index} has the value {value} which is out of bounds for vertex buffers of {max} length"
    )]
    IndexOutOfBounds { index: usize, value: u32, max: usize },
}

/// Easy to use builder for a [`Mesh`] that deals with common operations for
/// you.
#[derive(Debug, Default)]
pub struct MeshBuilder {
    vertex_positions: Vec<Vec3>,
    vertex_normals: Option<Vec<Vec3>>,
    vertex_tangents: Option<Vec<Vec3>>,
    vertex_uv0: Option<Vec<Vec2>>,
    vertex_uv1: Option<Vec<Vec2>>,
    vertex_colors: Option<Vec<[u8; 4]>>,
    vertex_joint_indices: Option<Vec<[u16; 4]>>,
    vertex_joint_weights: Option<Vec<Vec4>>,
    vertex_count: usize,

    indices: Option<Vec<u32>>,
    without_validation: bool,

    handedness: Handedness,
    flip_winding_order: bool,
    double_sided: bool,
}
impl MeshBuilder {
    /// Create a new [`MeshBuilder`] with a given set of positions.
    ///
    /// All vertices must have positions.
    pub fn new(vertex_positions: Vec<Vec3>, handedness: Handedness) -> Self {
        Self {
            vertex_count: vertex_positions.len(),
            vertex_positions,
            handedness,
            ..Self::default()
        }
    }

    /// Add vertex normals to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_normals(mut self, normals: Vec<Vec3>) -> Self {
        self.vertex_normals = Some(normals);
        self
    }

    /// Add vertex tangents to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_tangents(mut self, tangents: Vec<Vec3>) -> Self {
        self.vertex_tangents = Some(tangents);
        self
    }

    /// Add the first set of texture coordinates to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_uv0(mut self, uvs: Vec<Vec2>) -> Self {
        self.vertex_uv0 = Some(uvs);
        self
    }

    /// Add the second set of texture coordinates to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_uv1(mut self, uvs: Vec<Vec2>) -> Self {
        self.vertex_uv1 = Some(uvs);
        self
    }

    /// Add vertex colors to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_colors(mut self, colors: Vec<[u8; 4]>) -> Self {
        self.vertex_colors = Some(colors);
        self
    }

    /// Add vertex joint indices to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_joint_indices(mut self, joint_indices: Vec<[u16; 4]>) -> Self {
        self.vertex_joint_indices = Some(joint_indices);
        self
    }

    /// Add vertex joint weights to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_joint_weights(mut self, joint_weights: Vec<Vec4>) -> Self {
        self.vertex_joint_weights = Some(joint_weights);
        self
    }

    /// Add indices to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is zero.
    pub fn with_indices(mut self, indices: Vec<u32>) -> Self {
        self.indices = Some(indices);
        self
    }

    /// Flip the winding order
    ///
    /// See [`Mesh::flip_winding_order`] for more information.
    pub fn with_flip_winding_order(mut self) -> Self {
        self.flip_winding_order = true;
        self
    }

    /// Mark this mesh as needing to be double sided. This will duplicate all
    /// faces with the opposite winding order. This acts as if backface culling
    /// was disabled.
    pub fn with_double_sided(mut self) -> Self {
        self.double_sided = true;
        self
    }

    /// Doesn't run validation on the mesh.
    ///
    /// # Safety
    ///
    /// This asserts the following are true about the mesh:
    /// - All vertex arrays are the same length.
    /// - There is a non-zero count of vertices.
    /// - The count of vertices is less than [`MAX_VERTEX_COUNT`].
    /// - All indexes are in bounds for the given vertex arrays.
    /// - There is a non-zero count of indices.
    /// - There is a multiple-of-three count of indices.
    pub unsafe fn without_validation(mut self) -> Self {
        self.without_validation = true;
        self
    }

    /// Build a mesh, adding whatever components weren't provided.
    ///
    /// If normals weren't provided, they will be calculated. If mesh
    /// is right handed, will be converted to left handed.
    ///
    /// All others will be filled with defaults.
    pub fn build(self) -> Result<Mesh, MeshValidationError> {
        let length = self.vertex_count;

        let has_normals = self.vertex_normals.is_some();
        let has_tangents = self.vertex_tangents.is_some();
        let has_uvs = self.vertex_uv0.is_some();

        let mut mesh = Mesh {
            vertex_positions: self.vertex_positions,
            vertex_normals: self.vertex_normals.unwrap_or_else(|| vec![Vec3::ZERO; length]),
            vertex_tangents: self.vertex_tangents.unwrap_or_else(|| vec![Vec3::ZERO; length]),
            vertex_uv0: self.vertex_uv0.unwrap_or_else(|| vec![Vec2::ZERO; length]),
            vertex_uv1: self.vertex_uv1.unwrap_or_else(|| vec![Vec2::ZERO; length]),
            vertex_colors: self.vertex_colors.unwrap_or_else(|| vec![[255; 4]; length]),
            vertex_joint_indices: self.vertex_joint_indices.unwrap_or_else(|| vec![[0; 4]; length]),
            vertex_joint_weights: self.vertex_joint_weights.unwrap_or_else(|| vec![Vec4::ZERO; length]),
            indices: self.indices.unwrap_or_else(|| (0..length as u32).collect()),
        };

        if !self.without_validation {
            mesh.validate()?;
        }

        // We need to flip winding order first, so the normals will be facing the right
        // direction.
        if self.flip_winding_order {
            mesh.flip_winding_order();
        }

        if !has_normals {
            // SAFETY: We've validated this mesh or had its validity unsafely asserted.
            unsafe { mesh.calculate_normals(self.handedness, true) };
        }

        // Don't need to bother with tangents if there are no meaningful UVs
        if !has_tangents && has_uvs {
            // SAFETY: We've validated this mesh or had its validity unsafely asserted.
            unsafe { mesh.calculate_tangents(true) };
        }

        Ok(mesh)
    }
}

/// A mesh that may be used by many objects.
///
/// Meshes are in Structure of Array format and must have all the vertex_*
/// arrays be the same length. This condition can be checked with the
/// [`Mesh::validate`] function.
///
/// These can be annoying to construct, so use the [`MeshBuilder`] to make it
/// easier.
#[derive(Debug)]
pub struct Mesh {
    pub vertex_positions: Vec<Vec3>,
    pub vertex_normals: Vec<Vec3>,
    pub vertex_tangents: Vec<Vec3>,
    pub vertex_uv0: Vec<Vec2>,
    pub vertex_uv1: Vec<Vec2>,
    pub vertex_colors: Vec<[u8; 4]>,
    pub vertex_joint_indices: Vec<[u16; 4]>,
    pub vertex_joint_weights: Vec<Vec4>,

    pub indices: Vec<u32>,
}

impl Clone for Mesh {
    fn clone(&self) -> Self {
        Self {
            vertex_positions: self.vertex_positions.clone(),
            vertex_normals: self.vertex_normals.clone(),
            vertex_tangents: self.vertex_tangents.clone(),
            vertex_uv0: self.vertex_uv0.clone(),
            vertex_uv1: self.vertex_uv1.clone(),
            vertex_colors: self.vertex_colors.clone(),
            vertex_joint_indices: self.vertex_joint_indices.clone(),
            vertex_joint_weights: self.vertex_joint_weights.clone(),
            indices: self.indices.clone(),
        }
    }
}

impl Mesh {
    /// Validates that all vertex attributes have the same length.
    pub fn validate(&self) -> Result<(), MeshValidationError> {
        let position_length = self.vertex_positions.len();
        let indices_length = self.indices.len();

        if position_length > MAX_VERTEX_COUNT {
            return Err(MeshValidationError::ExceededMaxVertexCount { count: position_length });
        }

        let first_different_length = [
            (self.vertex_normals.len(), VertexBufferType::Normal),
            (self.vertex_tangents.len(), VertexBufferType::Tangent),
            (self.vertex_uv0.len(), VertexBufferType::Uv0),
            (self.vertex_uv1.len(), VertexBufferType::Uv1),
            (self.vertex_colors.len(), VertexBufferType::Colors),
        ]
        .iter()
        .find_map(|&(len, ty)| if len != position_length { Some((len, ty)) } else { None });

        if let Some((len, ty)) = first_different_length {
            return Err(MeshValidationError::MismatchedVertexCount {
                ty,
                actual: len,
                expected: position_length,
            });
        }

        if indices_length % 3 != 0 {
            return Err(MeshValidationError::IndexCountNotMultipleOfThree { count: indices_length });
        }

        let first_oob_index = self.indices.iter().enumerate().find_map(|(idx, &i)| {
            if (i as usize) >= position_length {
                Some((idx, i))
            } else {
                None
            }
        });

        if let Some((index, value)) = first_oob_index {
            return Err(MeshValidationError::IndexOutOfBounds {
                index,
                value,
                max: position_length,
            });
        }

        Ok(())
    }

    /// Calculate normals for the given mesh, assuming smooth shading and
    /// per-vertex normals.
    ///
    /// It is sound to call this function with the wrong handedness, it will
    /// just result in flipped normals.
    ///
    /// If zeroed is true, the normals will not be zeroed before hand. If this
    /// is falsely set, it is sound, just returns incorrect results.
    ///
    /// # Safety
    ///
    /// The following must be true:
    /// - Normals and positions must be the same length.
    /// - All indices must be in-bounds for the buffers.
    ///
    /// If a mesh has passed a call to validate, it is sound to call this
    /// function.
    pub unsafe fn calculate_normals(&mut self, handedness: Handedness, zeroed: bool) {
        if handedness == Handedness::Left {
            Self::calculate_normals_for_buffers::<true>(
                &mut self.vertex_normals,
                &self.vertex_positions,
                &self.indices,
                zeroed,
            )
        } else {
            Self::calculate_normals_for_buffers::<false>(
                &mut self.vertex_normals,
                &self.vertex_positions,
                &self.indices,
                zeroed,
            )
        }
    }

    /// Calculate normals for the given buffers representing a mesh, assuming
    /// smooth shading and per-vertex normals.
    ///
    /// It is sound to call this function with the wrong handedness, it will
    /// just result in flipped normals.
    ///
    /// If zeroed is true, the normals will not be zeroed before hand. If this
    /// is falsely set, it is safe, just returns incorrect results.
    ///
    /// # Safety
    ///
    /// The following must be true:
    /// - Normals and positions must be the same length.
    /// - All indices must be in-bounds for the buffers.
    ///
    /// If a mesh has passed a call to validate, it is sound to call this
    /// function.
    pub unsafe fn calculate_normals_for_buffers<const LEFT_HANDED: bool>(
        normals: &mut [Vec3],
        positions: &[Vec3],
        indices: &[u32],
        zeroed: bool,
    ) {
        debug_assert_eq!(normals.len(), positions.len());

        if !zeroed {
            for norm in normals.iter_mut() {
                *norm = Vec3::ZERO;
            }
        }

        for idx in indices.chunks_exact(3) {
            let (idx0, idx1, idx2) = match *idx {
                [idx0, idx1, idx2] => (idx0, idx1, idx2),
                // SAFETY: This is guaranteed by chunks_exact(3)
                _ => std::hint::unreachable_unchecked(),
            };

            // SAFETY: The conditions of this function assert all thes indices are in-bounds
            let pos1 = *positions.get_unchecked(idx0 as usize);
            let pos2 = *positions.get_unchecked(idx1 as usize);
            let pos3 = *positions.get_unchecked(idx2 as usize);

            let edge1 = pos2 - pos1;
            let edge2 = pos3 - pos1;

            let normal = if LEFT_HANDED {
                edge1.cross(edge2)
            } else {
                edge2.cross(edge1)
            };

            // SAFETY: The conditions of this function assert all thes indices are in-bounds
            *normals.get_unchecked_mut(idx0 as usize) += normal;
            *normals.get_unchecked_mut(idx1 as usize) += normal;
            *normals.get_unchecked_mut(idx2 as usize) += normal;
        }

        for normal in normals.iter_mut() {
            *normal = normal.normalize_or_zero();
        }
    }

    /// Calculate tangents for the given mesh, based on normals and texture
    /// coordinates.
    ///
    /// If zeroed is true, the normals will not be zeroed before hand. If this
    /// is falsely set, it is safe, just returns incorrect results.
    ///
    /// # Safety
    ///
    /// The following must be true:
    /// - Tangents, positions, normals, and uvs must be the same length.
    /// - All indices must be in-bounds for the buffers.
    ///
    /// If a mesh has passed a call to validate, it is sound to call this
    /// function.
    pub unsafe fn calculate_tangents(&mut self, zeroed: bool) {
        // SAFETY: The mesh unconditionally has a validation token, so it must be valid.
        Self::calculate_tangents_for_buffers(
            &mut self.vertex_tangents,
            &self.vertex_positions,
            &self.vertex_normals,
            &self.vertex_uv0,
            &self.indices,
            zeroed,
        )
    }

    /// Calculate tangents for the given set of buffers, based on normals and
    /// texture coordinates.
    ///
    /// If zeroed is true, the normals will not be zeroed before hand. If this
    /// is falsely set, it is safe, just returns incorrect results.
    ///
    /// # Safety
    ///
    /// The following must be true:
    /// - Tangents, positions, normals, and uvs must be the same length.
    /// - All indices must be in-bounds for the buffers.
    pub unsafe fn calculate_tangents_for_buffers(
        tangents: &mut [Vec3],
        positions: &[Vec3],
        normals: &[Vec3],
        uvs: &[Vec2],
        indices: &[u32],
        zeroed: bool,
    ) {
        debug_assert_eq!(tangents.len(), positions.len());
        debug_assert_eq!(uvs.len(), positions.len());

        if !zeroed {
            for tan in tangents.iter_mut() {
                *tan = Vec3::ZERO;
            }
        }

        for idx in indices.chunks_exact(3) {
            let (idx0, idx1, idx2) = match *idx {
                [idx0, idx1, idx2] => (idx0, idx1, idx2),
                // SAFETY: This is guaranteed by chunks_exact(3)
                _ => std::hint::unreachable_unchecked(),
            };

            // SAFETY: The conditions of this function assert all thes indices are in-bounds
            let pos1 = *positions.get_unchecked(idx0 as usize);
            let pos2 = *positions.get_unchecked(idx1 as usize);
            let pos3 = *positions.get_unchecked(idx2 as usize);

            let tex1 = *uvs.get_unchecked(idx0 as usize);
            let tex2 = *uvs.get_unchecked(idx1 as usize);
            let tex3 = *uvs.get_unchecked(idx2 as usize);

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

            // SAFETY: The conditions of this function assert all thes indices are in-bounds
            *tangents.get_unchecked_mut(idx0 as usize) += tangent;
            *tangents.get_unchecked_mut(idx1 as usize) += tangent;
            *tangents.get_unchecked_mut(idx2 as usize) += tangent;
        }

        for (tan, norm) in tangents.iter_mut().zip(normals) {
            let t = *tan - (*norm * norm.dot(*tan));
            *tan = t.normalize_or_zero();
        }
    }

    /// Converts the mesh from single sided to double sided.
    pub fn double_side(&mut self) {
        let starting_len = self.indices.len();
        // This floors, so the following unsafe is in-bounds.
        let primative_count = starting_len / 3;
        // reserve additional space -- this "doubles" the capasity
        self.indices.reserve(starting_len);

        let ptr = self.indices.as_mut_ptr();

        #[allow(clippy::identity_op)]
        unsafe {
            // Iterate in reverse as to not stomp on ourself
            for prim in (0..primative_count).rev() {
                let i1 = *ptr.add(prim * 3 + 0);
                let i2 = *ptr.add(prim * 3 + 1);
                let i3 = *ptr.add(prim * 3 + 2);

                // One triangle forward.
                ptr.add(prim * 6 + 0).write(i1);
                ptr.add(prim * 6 + 1).write(i2);
                ptr.add(prim * 6 + 2).write(i3);

                // One triangle reverse.
                ptr.add(prim * 6 + 3).write(i3);
                ptr.add(prim * 6 + 4).write(i2);
                ptr.add(prim * 6 + 5).write(i1);
            }

            self.indices.set_len(starting_len * 2);
        }
    }

    /// Inverts the winding order of a mesh. This is useful if you have meshes
    /// which are designed for right-handed (Counter-Clockwise) winding
    /// order for use in OpenGL or VK.
    ///
    /// This does not change vertex location, so does not change coordinate
    /// system. This will also not change the vertex normals. Calling
    /// [`Mesh::calculate_normals`] is advised after calling this function.
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

/// The count of mipmap levels a texture should have.
#[derive(Debug, Clone)]
pub enum MipmapCount {
    /// Specifies a texture with the tiven mipmap count. Must not be greater
    /// than the maximum.
    Specific(NonZeroU32),
    /// Specifies a texture with the maximum mipmap count.
    Maximum,
}

impl MipmapCount {
    pub const ONE: Self = Self::Specific(unsafe { NonZeroU32::new_unchecked(1) });
}

/// How texture mipmaps get generated.
#[derive(Debug, Clone)]
pub enum MipmapSource {
    /// The user will provide all of the mipmaps in the data texture. Upload all
    /// mip levels.
    Uploaded,
    /// rend3 will generate the mipmaps for you. Upload only mip level 0.
    Generated,
}

/// A bitmap image used as a data source for a texture.
#[derive(Debug, Clone)]
pub struct Texture {
    pub label: Option<String>,
    pub data: Vec<u8>,
    pub format: TextureFormat,
    pub size: UVec2,
    pub mip_count: MipmapCount,
    pub mip_source: MipmapSource,
}

/// Describes a texture made from the mipmaps of another texture.
#[derive(Debug, Clone)]
pub struct TextureFromTexture {
    pub label: Option<String>,
    pub src: TextureHandle,
    pub start_mip: u32,
    pub mip_count: Option<NonZeroU32>,
}

#[doc(hidden)]
pub struct MaterialTag;

/// Interface that all materials must use.
///
/// The material will provide a set of textures, and a pile of bytes. It will
/// then, as part of the material bind group, present the following abi:
///
/// ### CpuDriven Profile
///
/// - A uniform binding with:
///   - The data provided, with padding up to 16 byte alignment.
///   - A u32 bitflag telling which textures are null. To check if texture N is
///     enabled, do `(texture_bitflag >> N) & 0x1 == 1`.
/// - One Texture2D binding per texture, provided in the order given. If given a
///   `None`, will bind a null texture (1x1 texture with a (0, 0, 0, 255)
///   pixel).
///
/// ### GpuDriven Profile
/// - A material array indexed by the material index. Each material has:
///   - One u32 per texture. If this value is 0, the texture doesn't exist. If
///     this value is non-zero, subtract one and index into the texture array to
///     ge thte texture.
///   - Padding to 16 byte alignemnet.
///   - The data provided by the material.
pub trait Material: Send + Sync + 'static {
    /// The texture count that will be provided to `to_textures`.
    const TEXTURE_COUNT: u32;
    /// The amount of data that will be provided to `to_data`.
    const DATA_SIZE: u32;

    /// u64 key that determine's an object's archetype. When you query for
    /// objects from the object manager, you must provide this key to get all
    /// objects with this key.
    fn object_key(&self) -> u64;

    /// Fill up the given slice with textures.
    fn to_textures<'a>(&'a self, slice: &mut [Option<&'a TextureHandle>]);

    /// Fill up the given slice with binary material data. This can be whatever
    /// data a shader expects.
    fn to_data(&self, slice: &mut [u8]);
}

/// Source of a mesh for an object.
#[derive(Clone, Debug)]
pub enum ObjectMeshKind {
    Animated(SkeletonHandle),
    Static(MeshHandle),
}

changeable_struct! {
    /// An object in the world that is composed of a [`Mesh`] and [`Material`].
    pub struct Object <- ObjectChange {
        pub mesh_kind: ObjectMeshKind,
        pub material: MaterialHandle,
        pub transform: Mat4,
    }
}

/// Describes how the camera should look at the scene.
#[derive(Debug, Default, Copy, Clone)]
pub struct Camera {
    pub projection: CameraProjection,
    /// View matrix
    pub view: Mat4,
}

/// Describes how the world should be projected into the camera.
#[derive(Debug, Copy, Clone)]
pub enum CameraProjection {
    Orthographic {
        /// Size assumes the location is at the center of the camera area.
        size: Vec3A,
    },
    Perspective {
        /// Vertical field of view in degrees.
        vfov: f32,
        /// Near plane distance. All projection uses a infinite far plane.
        near: f32,
    },
    Raw(Mat4),
}

impl Default for CameraProjection {
    fn default() -> Self {
        Self::Perspective { vfov: 60.0, near: 0.1 }
    }
}

changeable_struct! {
    /// Describes how directional lights (sun lights) and their shadows should be processed.
    pub struct DirectionalLight <- DirectionalLightChange {
        /// Color of the light.
        pub color: Vec3,
        /// Constant multiplier for the light.
        pub intensity: f32,
        /// Direction of the sun.
        pub direction: Vec3,
        /// Distance from the camera that shadows should be calculated.
        pub distance: f32,
    }
}

/// The sample count when doing multisampling.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SampleCount {
    One = 1,
    Four = 4,
}

impl Default for SampleCount {
    fn default() -> Self {
        Self::One
    }
}

impl TryFrom<u8> for SampleCount {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::One,
            4 => Self::Four,
            v => return Err(v),
        })
    }
}

impl SampleCount {
    /// Determines if a resolve texture is needed for this texture.
    pub fn needs_resolve(self) -> bool {
        self != Self::One
    }
}

/// Describes the "Handedness" of a given coordinate system. Affects math done
/// in the space.
///
/// While a weird term, if you make your thumb X, your pointer Y,
/// and your middle finger Z, the handedness can be determined by which hand can
/// contort to represent the coordinate system.
///
/// For example  
/// +X right, +Y up, +Z _into_ the screen is left handed.  
/// +X right, +Y up, +Z _out of_ the screen is right handed.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Handedness {
    Left,
    Right,
}

impl Default for Handedness {
    fn default() -> Self {
        Self::Left
    }
}

/// A Skeleton stores the necessary data to do vertex skinning for an [Object].
#[derive(Debug, Clone)]
pub struct Skeleton {
    /// Stores one transformation matrix for each joint. These are the
    /// transformations that will be applied to the vertices affected by the
    /// corresponding joint. Not to be confused with the transform matrix of the
    /// joint itself.
    ///
    /// The `Skeleton::form_joint_transforms` constructor can be used to create
    /// a Skeleton with the joint transform matrices instead.
    pub joint_matrices: Vec<Mat4>,
    pub mesh: MeshHandle,
}

impl Skeleton {
    /// Creates a skeleton with the list of global transforms and inverse bind
    /// transforms for each joint
    pub fn from_joint_transforms(
        mesh: MeshHandle,
        joint_global_transforms: &[Mat4],
        inverse_bind_transforms: &[Mat4],
    ) -> Skeleton {
        let joint_matrices = Self::compute_joint_matrices(joint_global_transforms, inverse_bind_transforms);
        Skeleton { joint_matrices, mesh }
    }

    /// Given a list of joint global positions and another one with inverse bind
    /// matrices, multiplies them together to return the list of joint matrices.
    pub fn compute_joint_matrices(joint_global_transforms: &[Mat4], inverse_bind_transforms: &[Mat4]) -> Vec<Mat4> {
        joint_global_transforms
            .iter()
            .zip(inverse_bind_transforms.iter())
            .map(|(global_pos, inv_bind_pos)| (*global_pos) * (*inv_bind_pos))
            .collect()
    }
}
