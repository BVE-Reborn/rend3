#![warn(unsafe_op_in_unsafe_fn)]

//! Type declarations for the rend3 3D rendering crate.
//!
//! This is reexported in the rend3 crate proper and includes all the "surface"
//! api arguments.

use std::{
    any::Any,
    fmt::Debug,
    hash::Hash,
    marker::PhantomData,
    mem::{self, size_of},
    num::NonZeroU32,
    ops::Deref,
    slice,
    sync::Arc,
};

use bytemuck::Zeroable;
/// Reexport of the glam version rend3 is using.
pub use glam;
use glam::{Mat4, UVec2, Vec2, Vec3, Vec3A, Vec4};
use thiserror::Error;

mod attribute;
pub use attribute::*;

/// Non-owning resource handle.
///
/// Not part of rend3's external interface, but needed to interface with rend3's
/// internal datastructures if writing your own structures or render routines.
pub struct RawResourceHandle<T> {
    /// Underlying value of the handle.
    pub idx: usize,
    _phantom: PhantomData<T>,
}

impl<T> RawResourceHandle<T> {
    /// Creates a new handle with the given value
    pub const fn new(idx: usize) -> Self {
        Self { idx, _phantom: PhantomData }
    }
}

// Need Debug/Copy/Clone impls that don't require T: Trait.
impl<T> Debug for RawResourceHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawResourceHandle").field("idx", &self.idx).finish()
    }
}
/// ***IT IS UNSAFE THAT THIS IS COPYABLE***
/// This may result in a RawResourceHandle outliving the index entry to which it points.
/// ***NEEDS WORK***
////// /*
impl<T> Copy for RawResourceHandle<T> {}

impl<T> Clone for RawResourceHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}
////// */
impl<T> PartialEq for RawResourceHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<T> Eq for RawResourceHandle<T> {}

impl<T> Hash for RawResourceHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
    }
}
/*
/// Function called to deindex a resource handle.
#[cfg(not(target_arch = "wasm32"))]
type ResourceDeindexer: dyn Fn(RawResourceHandle<T>) + Send + Sync;
#[cfg(target_arch = "wasm32")]
type ResourceDeindexer: dyn Fn(RawResourceHandle<T>);
*/
/// Ref counter for RawResourceHandle inside of ResourceHandle
///
/// Needed to avoid a race condition at de-indexing.
///
/// Soundness:
/// - Deindexer can be called only once - yes
//  - Deindexer will always be called on drop - yes.
struct ResourceHandleRefcount<T> {
    //////deindexer: ResourceDeindexer
    /// Index to the resource
    raw: RawResourceHandle<T>,
    /// Called at drop to delete the item from the index.
    /// This needs a Box or Arc because the closure is a variable-sized object.
    /// Arc would be better if the callers generated an Arc, because it could be a cheap clone.
    #[cfg(not(target_arch = "wasm32"))]
    deindexer: Box<dyn Fn(&RawResourceHandle<T>) + Send + Sync + 'static>,
    #[cfg(target_arch = "wasm32")]
    deindexer: Box<dyn Fn(&RawResourceHandle<T>) + 'static>,

}

impl<T> ResourceHandleRefcount<T> {
    /// Usual new fn
    pub fn new(destroy_fn: impl Fn(&RawResourceHandle<T>) + WasmNotSend + WasmNotSync + 'static, idx: usize) -> Self {
        Self {
            deindexer: Box::new(destroy_fn),
            raw: RawResourceHandle { idx, _phantom: PhantomData },
        }
    }
    
    /// Get the raw resource handle
    pub fn get_raw(&self) -> &RawResourceHandle<T>{
        &self.raw
    }
}

impl<T> Drop for ResourceHandleRefcount<T> {
    /// When this is dropped, deindex.
    fn drop(&mut self) {
        (self.deindexer)(&self.raw);
    }
}

impl<T> Debug for ResourceHandleRefcount<T> {
    /// Debug formatting
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceHandleRefcount")
            //////.field("refcount", &Arc::strong_count(&self.refcount))
            .field("idx", &self.raw.idx)
            .finish()
    }
}

/// Owning resource handle. Used as part of rend3's interface.
pub struct ResourceHandle<T> {
    /// Inside this arc is the function to call when
    /// this resource handle is destroyed and we
    /// need to phone home. We're just reusing
    /// the allocation for both refcount and function
    /// purposes.
/*
    #[cfg(not(target_arch = "wasm32"))]
    refcount: Arc<dyn Fn(RawResourceHandle<T>) + Send + Sync>,
    #[cfg(target_arch = "wasm32")]
    refcount: Arc<dyn Fn(RawResourceHandle<T>)>,
    raw: RawResourceHandle<T>,
*/
    /// Deindexer and its index
    refcount: Arc<ResourceHandleRefcount<T>>,
    /// Dummy to prevent using wrong type
    _phantom: PhantomData<T>,
}
/*
impl<T> Drop for ResourceHandle<T> {
    fn drop(&mut self) {
        if Arc::strong_count(&self.refcount) == 1 {
            (self.refcount)(self.raw);
        }
    }
}
*/

impl<T> Debug for ResourceHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceHandle")
            .field("refcount", &Arc::strong_count(&self.refcount))
            .field("idx", &self.get_raw().idx)
            .finish()
    }
}
impl<T> Clone for ResourceHandle<T> {
    fn clone(&self) -> Self {
        Self { refcount: self.refcount.clone(), _phantom: self._phantom }
    }
}

impl<T> PartialEq for ResourceHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.get_raw().idx == other.refcount.get_raw().idx
    }
}

impl<T> Eq for ResourceHandle<T> {}

impl<T> Hash for ResourceHandle<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.get_raw().idx.hash(state);
    }
}

impl<T> ResourceHandle<T> {
    /// Create a new resource handle from an index.
    ///
    /// Part of rend3's internal interface, use `Renderer::add_*` instead.
    pub fn new(destroy_fn: impl Fn(&RawResourceHandle<T>) + WasmNotSend + WasmNotSync + 'static, idx: usize) -> Self {
        Self {
            refcount: Arc::new(ResourceHandleRefcount::new(destroy_fn, idx)),
            //////refcount: Arc::new(destroy_fn),
            //////raw: RawResourceHandle { idx, _phantom: PhantomData },
            _phantom: PhantomData,
        }
    }

    /// Gets the equivalent raw handle for this owning handle.
    ///
    /// Part of rend3's internal interface for accessing internal resrouces
    pub fn get_raw(&self) -> &RawResourceHandle<T> {
        &self.refcount.raw
    }
}

impl<T> Deref for ResourceHandle<T> {
    type Target = RawResourceHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.refcount.raw
    }
}

/// Tag type for differentiating Texture2Ds on the type level.
#[doc(hidden)]
pub struct Texture2DTag;
/// Tag type for differentiating TextureCubes on the type level.
#[doc(hidden)]
pub struct TextureCubeTag;
/// Tag type for differentiating Materials on the type level.
#[doc(hidden)]
pub struct MaterialTag;
/// Tag type for differentiating GraphData on the type level.
#[doc(hidden)]
pub struct GraphDataTag;

/// Refcounted handle to a Mesh
pub type MeshHandle = ResourceHandle<Mesh>;
/// Refcounted handle to a Texture2D
pub type Texture2DHandle = ResourceHandle<Texture2DTag>;
/// Refcounted handle to a TextureCube
pub type TextureCubeHandle = ResourceHandle<TextureCubeTag>;
/// Refcounted handle to a Material
pub type MaterialHandle = ResourceHandle<MaterialTag>;
/// Refcounted handle to an Object
pub type ObjectHandle = ResourceHandle<Object>;
/// Refcounted handle to a DirectionalLight
pub type DirectionalLightHandle = ResourceHandle<DirectionalLight>;
/// Refcounted handle to a PointLight
pub type PointLightHandle = ResourceHandle<PointLight>;
/// Refcounted handle to a Skeleton
pub type SkeletonHandle = ResourceHandle<Skeleton>;
/// Refcounted handle to an instance of GraphData with the type erased
pub type GraphDataHandleUntyped = ResourceHandle<GraphDataTag>;
/// Refcounted handle to an instance of GraphData
pub struct GraphDataHandle<T>(pub GraphDataHandleUntyped, pub PhantomData<T>);

impl<T> Debug for GraphDataHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Clone for GraphDataHandle<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone(), PhantomData)
    }
}

/// Internal non-owning handle to a Mesh
pub type RawMeshHandle = RawResourceHandle<Mesh>;
/// Internal non-owning handle to a Texture2D
pub type RawTexture2DHandle = RawResourceHandle<Texture2DTag>;
/// Internal non-owning handle to a TextureCube
pub type RawTextureCubeHandle = RawResourceHandle<TextureCubeTag>;
/// Internal non-owning handle to a Material
pub type RawMaterialHandle = RawResourceHandle<MaterialTag>;
/// Internal non-owning handle to an Object
pub type RawObjectHandle = RawResourceHandle<Object>;
/// Internal non-owning handle to a DirectionalLight
pub type RawDirectionalLightHandle = RawResourceHandle<DirectionalLight>;
/// Internal non-owning handle to a PointLight
pub type RawPointLightHandle = RawResourceHandle<PointLight>;
/// Internal non-owning handle to a Skeleton
pub type RawSkeletonHandle = RawResourceHandle<Skeleton>;
/// Internal non-owning handle to an instance of GraphData with the type erased
pub type RawGraphDataHandleUntyped = RawResourceHandle<GraphDataTag>;
/// Internal non-owning handle to an instance of GraphData
pub struct RawGraphDataHandle<T>(pub RawGraphDataHandleUntyped, pub PhantomData<T>);

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
pub use wgpu_types as wgt;
pub use wgt::{
    AstcBlock, AstcChannel, Backend, Backends, Color, DeviceType, PresentMode, TextureFormat,
    TextureFormatFeatureFlags, TextureUsages,
};

/// The maximum amount of vertices any one object can have.
///
/// The value allows for 8 bits of information packed in the high 8 bits of the
/// index for object recombination.
///
/// We leave exactly one value at the top for the "invalid vertex" value: 0x00_FF_FF_FF;
pub const MAX_VERTEX_COUNT: u32 = (1 << 24) - 1;
/// The maximum amount of indices any one object can have.
pub const MAX_INDEX_COUNT: u32 = u32::MAX;

/// Error returned from mesh validation.
#[derive(Debug, Error)]
pub enum MeshValidationError {
    #[error("Mesh's {:?} buffer has {actual} vertices but the position buffer has {expected}", .attribute_id.name())]
    MismatchedVertexCount { attribute_id: &'static VertexAttributeId, expected: usize, actual: usize },
    #[error("Mesh has {count} vertices when the vertex limit is {MAX_VERTEX_COUNT}")]
    ExceededMaxVertexCount { count: usize },
    #[error("Mesh has {count} indicies when maximum index count is {MAX_INDEX_COUNT}")]
    ExceededMaxIndexCount { count: usize },
    #[error("Mesh has {count} indices which is not a multiple of three. Meshes are always composed of triangles")]
    IndexCountNotMultipleOfThree { count: usize },
    #[error(
        "Index at position {index} has the value {value} which is out of bounds for vertex buffers of {max} length"
    )]
    IndexOutOfBounds { index: usize, value: u32, max: u32 },
}

#[derive(Debug)]
pub struct StoredVertexAttributeData {
    id: &'static VertexAttributeId,
    data: WasmVecAny,
    ptr: *const u8,
    bytes: u64,
}
impl StoredVertexAttributeData {
    pub fn new<T>(attribute: &'static VertexAttribute<T>, data: Vec<T>) -> Self
    where
        T: VertexFormat,
    {
        let bytes = (data.len() * size_of::<T>()) as u64;
        let ptr = data.as_ptr() as *const u8;
        Self { id: attribute.id(), data: WasmVecAny::from(data), ptr, bytes }
    }

    pub fn id(&self) -> &'static VertexAttributeId {
        self.id
    }

    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn untyped_data(&self) -> &[u8] {
        // SAFETY: the pointer is to the vector's allocation which is still live and will be for the length of 'self.
        //         the length is the exact byte length of the allocation.
        unsafe { slice::from_raw_parts(self.ptr, self.bytes as usize) }
    }

    /// Gets the typed data if the attributes match ids and have the same types.
    pub fn typed_data<T: VertexFormat>(&self, attribute: &'static VertexAttribute<T>) -> Option<&[T]> {
        if attribute.id() != self.id {
            return None;
        }
        Some(self.data.downcast_slice::<T>().unwrap())
    }

    pub fn typed_data_mut<T: VertexFormat>(&mut self, attribute: &'static VertexAttribute<T>) -> Option<&mut [T]> {
        if attribute.id() != self.id {
            return None;
        }
        Some(self.data.downcast_slice_mut::<T>().unwrap())
    }
}

unsafe impl Send for StoredVertexAttributeData {}
unsafe impl Sync for StoredVertexAttributeData {}

/// Easy to use builder for a [`Mesh`] that deals with common operations for
/// you.
#[derive(Debug, Default)]
pub struct MeshBuilder {
    vertex_attributes: Vec<StoredVertexAttributeData>,
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
            vertex_attributes: vec![StoredVertexAttributeData::new(&VERTEX_ATTRIBUTE_POSITION, vertex_positions)],
            handedness,
            ..Self::default()
        }
    }

    pub fn with_attribute<T: VertexFormat>(mut self, attribute: &'static VertexAttribute<T>, values: Vec<T>) -> Self {
        self.vertex_attributes.push(StoredVertexAttributeData::new(attribute, values));
        self
    }

    /// Add vertex normals to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_normals(self, normals: Vec<Vec3>) -> Self {
        self.with_attribute(&VERTEX_ATTRIBUTE_NORMAL, normals)
    }

    /// Add vertex tangents to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_tangents(self, tangents: Vec<Vec3>) -> Self {
        self.with_attribute(&VERTEX_ATTRIBUTE_TANGENT, tangents)
    }

    /// Add the first set of texture coordinates to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_texture_coordinates_0(self, coords: Vec<Vec2>) -> Self {
        self.with_attribute(&VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_0, coords)
    }

    /// Add the second set of texture coordinates to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_texture_coordinates_1(self, coords: Vec<Vec2>) -> Self {
        self.with_attribute(&VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_1, coords)
    }

    /// Add vertex colors to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_color_0(self, colors: Vec<[u8; 4]>) -> Self {
        self.with_attribute(&VERTEX_ATTRIBUTE_COLOR_0, colors)
    }

    /// Add vertex joint indices to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_joint_indices(self, joint_indices: Vec<[u16; 4]>) -> Self {
        self.with_attribute(&VERTEX_ATTRIBUTE_JOINT_INDICES, joint_indices)
    }

    /// Add vertex joint weights to the given mesh.
    ///
    /// # Panic
    ///
    /// Will panic if the length is different from the position buffer length.
    pub fn with_vertex_joint_weights(self, joint_weights: Vec<Vec4>) -> Self {
        self.with_attribute(&VERTEX_ATTRIBUTE_JOINT_WEIGHTS, joint_weights)
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
        let mut mesh = Mesh {
            attributes: self.vertex_attributes,
            vertex_count: self.vertex_count,
            indices: self.indices.unwrap_or_else(|| (0..self.vertex_count as u32).collect()),
        };

        if self.double_sided {
            mesh.double_side();
        }

        let has_normals = mesh.find_attribute_index(&VERTEX_ATTRIBUTE_NORMAL).is_some();
        let has_tangents = mesh.find_attribute_index(&VERTEX_ATTRIBUTE_TANGENT).is_some();

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

        if !has_tangents {
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
    pub attributes: Vec<StoredVertexAttributeData>,
    pub vertex_count: usize,

    pub indices: Vec<u32>,
}

impl Mesh {
    /// Validates that all vertex attributes have the same length.
    pub fn validate(&self) -> Result<(), MeshValidationError> {
        let position_length = self.vertex_count;
        let indices_length = self.indices.len();

        if position_length > MAX_VERTEX_COUNT as usize {
            return Err(MeshValidationError::ExceededMaxVertexCount { count: position_length });
        }

        for attribute in &self.attributes {
            let attribute_len = attribute.data.len();
            if attribute_len != position_length {
                return Err(MeshValidationError::MismatchedVertexCount {
                    attribute_id: attribute.id(),
                    actual: attribute_len,
                    expected: position_length,
                });
            }
        }

        if indices_length % 3 != 0 {
            return Err(MeshValidationError::IndexCountNotMultipleOfThree { count: indices_length });
        }

        if indices_length >= MAX_INDEX_COUNT as usize {
            return Err(MeshValidationError::ExceededMaxIndexCount { count: indices_length });
        }

        for (index, &value) in self.indices.iter().enumerate() {
            if value as usize >= position_length {
                return Err(MeshValidationError::IndexOutOfBounds { index, value, max: position_length as u32 });
            }
        }

        Ok(())
    }

    /// Returns the index in to the attribute array for a given attribute. If
    /// there is no such attribute, returns None.
    pub fn find_attribute_index(&self, desired_attribute: &'static VertexAttributeId) -> Option<usize> {
        self.attributes
            .iter()
            .enumerate()
            .find_map(|(idx, attribute)| (attribute.id == desired_attribute).then_some(idx))
    }

    /// Returns the index in to the attribute array for a given attribute. Creates the attribute
    /// if the attribute is not found, filling it with zeros.
    ///
    /// Returns true if the attribute is newly created.
    pub fn find_or_create_attribute_index<T: VertexFormat>(
        &mut self,
        desired_attribute: &'static VertexAttribute<T>,
    ) -> (usize, bool) {
        let index = self.find_attribute_index(desired_attribute.id());

        index.map_or_else(
            || {
                let idx = self.attributes.len();
                self.attributes
                    .push(StoredVertexAttributeData::new(desired_attribute, vec![T::zeroed(); self.vertex_count]));
                // There were no normals, and our created normals are already zeroed.
                (idx, true)
            },
            |idx| (idx, false),
        )
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
        let (normals_index, normals_created) = self.find_or_create_attribute_index(&VERTEX_ATTRIBUTE_NORMAL);

        let (position_attribute, remaining_attributes) = self.attributes.split_first_mut().unwrap();
        let positions = position_attribute.typed_data(&VERTEX_ATTRIBUTE_POSITION).unwrap();
        let normals = remaining_attributes[normals_index - 1].typed_data_mut(&VERTEX_ATTRIBUTE_NORMAL).unwrap();

        if handedness == Handedness::Left {
            unsafe {
                Self::calculate_normals_for_buffers::<true>(
                    normals,
                    positions,
                    &self.indices,
                    zeroed || normals_created,
                )
            }
        } else {
            unsafe {
                Self::calculate_normals_for_buffers::<false>(
                    normals,
                    positions,
                    &self.indices,
                    zeroed || normals_created,
                )
            }
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
                _ => unsafe { std::hint::unreachable_unchecked() },
            };

            // SAFETY: The conditions of this function assert all thes indices are in-bounds
            let pos1 = unsafe { *positions.get_unchecked(idx0 as usize) };
            let pos2 = unsafe { *positions.get_unchecked(idx1 as usize) };
            let pos3 = unsafe { *positions.get_unchecked(idx2 as usize) };

            let edge1 = pos2 - pos1;
            let edge2 = pos3 - pos1;

            let normal = if LEFT_HANDED { edge1.cross(edge2) } else { edge2.cross(edge1) };

            // SAFETY: The conditions of this function assert all thes indices are in-bounds
            unsafe { *normals.get_unchecked_mut(idx0 as usize) += normal };
            unsafe { *normals.get_unchecked_mut(idx1 as usize) += normal };
            unsafe { *normals.get_unchecked_mut(idx2 as usize) += normal };
        }

        for normal in normals.iter_mut() {
            *normal = normal.normalize_or_zero();
        }
    }

    /// Calculate tangents for the given mesh, based on normals and texture
    /// coordinates.
    ///
    /// If either normals or uv_0 don't exist on the mesh, this will not generate tangents.
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
        let normal_index = match self.find_attribute_index(&VERTEX_ATTRIBUTE_NORMAL) {
            Some(i) => i,
            None => return,
        };
        let uv_0_index = match self.find_attribute_index(&VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_0) {
            Some(i) => i,
            None => return,
        };
        let (tangent_index, tangents_created) = self.find_or_create_attribute_index(&VERTEX_ATTRIBUTE_TANGENT);

        // Assert that all indices are disjoint. This should never
        // not be the case, but validate in debug to make sure it is the case.
        debug_assert_ne!(0, tangent_index);
        debug_assert_ne!(0, normal_index);
        debug_assert_ne!(0, uv_0_index);
        debug_assert_ne!(tangent_index, normal_index);
        debug_assert_ne!(tangent_index, uv_0_index);
        debug_assert_ne!(normal_index, uv_0_index);

        // Assert that all indices are in bounds.
        debug_assert!(self.attributes.first().is_some());
        debug_assert!(self.attributes.get(tangent_index).is_some());
        debug_assert!(self.attributes.get(normal_index).is_some());
        debug_assert!(self.attributes.get(uv_0_index).is_some());

        // SAFETY: These references never escape this function, the attributes array isn't modified, and all indices are disjoint.
        //
        // We only use unsafe because we need to split-borrow different members of the array.
        let attr_ptr = self.attributes.as_mut_ptr();
        let positions_ref = unsafe { &*attr_ptr.add(0) };
        let tangents_mut = unsafe { &mut *attr_ptr.add(tangent_index) };
        let normals_ref = unsafe { &*attr_ptr.add(normal_index) };
        let uv_0_ref = unsafe { &*attr_ptr.add(uv_0_index) };

        let positions = positions_ref.typed_data(&VERTEX_ATTRIBUTE_POSITION).unwrap();
        let tangents = tangents_mut.typed_data_mut(&VERTEX_ATTRIBUTE_TANGENT).unwrap();
        let normals = normals_ref.typed_data(&VERTEX_ATTRIBUTE_NORMAL).unwrap();
        let uv_0 = uv_0_ref.typed_data(&VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_0).unwrap();

        // SAFETY: This function's caller has the same requirements as this one.
        unsafe {
            Self::calculate_tangents_for_buffers(
                tangents,
                positions,
                normals,
                uv_0,
                &self.indices,
                zeroed || tangents_created,
            )
        };
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
                _ => unsafe { std::hint::unreachable_unchecked() },
            };

            // SAFETY: The conditions of this function assert all thes indices are in-bounds
            let pos1 = unsafe { *positions.get_unchecked(idx0 as usize) };
            let pos2 = unsafe { *positions.get_unchecked(idx1 as usize) };
            let pos3 = unsafe { *positions.get_unchecked(idx2 as usize) };

            let tex1 = unsafe { *uvs.get_unchecked(idx0 as usize) };
            let tex2 = unsafe { *uvs.get_unchecked(idx1 as usize) };
            let tex3 = unsafe { *uvs.get_unchecked(idx2 as usize) };

            let edge1 = pos2 - pos1;
            let edge2 = pos3 - pos1;

            let uv1 = tex2 - tex1;
            let uv2 = tex3 - tex1;

            let r = 1.0 / (uv1.x * uv2.y - uv1.y * uv2.x);

            let tangent = (edge1 * Vec3::splat(uv2.y)) - (edge2 * Vec3::splat(uv1.y)) * r;

            // SAFETY: The conditions of this function assert all thes indices are in-bounds
            unsafe { *tangents.get_unchecked_mut(idx0 as usize) += tangent };
            unsafe { *tangents.get_unchecked_mut(idx1 as usize) += tangent };
            unsafe { *tangents.get_unchecked_mut(idx2 as usize) += tangent };
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
#[derive(Debug)]
pub struct TextureFromTexture {
    pub label: Option<String>,
    pub src: RawTexture2DHandle,
    pub start_mip: u32,
    pub mip_count: Option<NonZeroU32>,
}

/// Description of how this object should be sorted.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Sorting {
    pub reason: SortingReason,
    pub order: SortingOrder,
}

impl Sorting {
    /// Default sorting for opaque and cutout objects
    pub const OPAQUE: Self = Self { reason: SortingReason::Optimization, order: SortingOrder::FrontToBack };

    /// Default sorting for any objects using blending
    pub const BLENDING: Self = Self { reason: SortingReason::Requirement, order: SortingOrder::BackToFront };
}

/// Reason why object need sorting
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SortingReason {
    /// Objects should be sorted for optimization purposes.
    Optimization,
    /// If objects aren't sorted, things will render incorrectly.
    Requirement,
}

/// An object sorting order.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SortingOrder {
    /// Sort with the nearest objects first.
    FrontToBack,
    /// Sort with the furthest objects first.
    BackToFront,
}

/// Trait that abstracts over all possible arrays of optional raw texture handles.
///
/// The IntoIterator stuff in this trait is because rust-analyzer gets totally
/// confused when multiple IntoIterator bounds are involved. If I were to have
/// `MaterialArray<T>: IntoIterator<Item = T>` like I would want, any into_iter
/// based iteration of any iterator with a material argument in it
/// (so `Iterator<Item = ObjectWrapper<M>>` for example) will completely stop being
/// type deduced by RA. So to work around this, this trait also needs to act as
/// IntoIterator. See <https://github.com/rust-lang/rust-analyzer/issues/11242>.
pub trait MaterialArray<T>: AsRef<[T]> {
    /// An array of the [u32; COUNT]. We need this internally
    /// for shader layout stuff.
    type U32Array: encase::ShaderSize
        + encase::internal::WriteInto
        + Debug
        + Zeroable
        + Clone
        + Copy
        + Send
        + Sync
        + 'static;
    type IntoIter: Iterator<Item = T>;
    const COUNT: u32;

    fn map_to_u32<F>(self, func: F) -> Self::U32Array
    where
        F: FnMut(T) -> u32;

    fn into_iter(self) -> Self::IntoIter;
}

impl<const C: usize, T> MaterialArray<T> for [T; C] {
    type U32Array = [u32; C];
    type IntoIter = <[T; C] as IntoIterator>::IntoIter;
    const COUNT: u32 = C as u32;

    fn map_to_u32<F>(self, func: F) -> Self::U32Array
    where
        F: FnMut(T) -> u32,
    {
        self.map(func)
    }

    fn into_iter(self) -> Self::IntoIter {
        <Self as IntoIterator>::into_iter(self)
    }
}

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
///     get the texture.
///   - Padding to 16 byte alignment.
///   - The data provided by the material.
pub trait Material: WasmNotSend + WasmNotSync + 'static {
    type DataType: encase::ShaderSize + encase::internal::WriteInto;
    type TextureArrayType: MaterialArray<Option<RawTexture2DHandle>>;
    type RequiredAttributeArrayType: MaterialArray<&'static VertexAttributeId>;
    type SupportedAttributeArrayType: MaterialArray<&'static VertexAttributeId>;

    fn required_attributes() -> Self::RequiredAttributeArrayType;
    fn supported_attributes() -> Self::SupportedAttributeArrayType;

    /// u64 key that allows different materials to be somehow categorized.
    fn key(&self) -> u64;

    /// How objects with this material should be sorted.
    fn sorting(&self) -> Sorting;

    /// The array of textures that should be bound. Rend3 supports up to 32.
    fn to_textures(&self) -> Self::TextureArrayType;

    /// Fill up the given slice with data. This can be whatever data the shader expects.
    fn to_data(&self) -> Self::DataType;
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
        /// Resolution of the shadow map cascades (in pix)
        pub resolution: u16,
        /// Constant multiplier for the light.
        pub intensity: f32,
        /// Direction of the sun.
        pub direction: Vec3,
        /// Distance from the camera that shadows should be calculated.
        pub distance: f32,
    }
}

changeable_struct! {
    /// Describes how point lights and their shadows should be processed.
    pub struct PointLight <- PointLightChange {
        /// The position of the light in the world.
        pub position: Vec3,

        /// The color of the light.
        pub color: Vec3,

        /// The radius of the light.
        pub radius: f32,

        /// Constant multiplier for the light.
        pub intensity: f32,
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
    pub const ARRAY: [Self; 2] = [Self::One, Self::Four];

    /// Determines if a resolve texture is needed for this texture.
    pub const fn needs_resolve(self) -> bool {
        !matches!(self, Self::One)
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

impl From<Handedness> for wgt::FrontFace {
    fn from(value: Handedness) -> Self {
        match value {
            Handedness::Left => Self::Cw,
            Handedness::Right => Self::Ccw,
        }
    }
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

#[cfg(target_arch = "wasm32")]
trait_supertrait_alias!(pub WasmNotSend: Any);
#[cfg(not(target_arch = "wasm32"))]
trait_supertrait_alias!(pub WasmNotSend: Any + Send);
#[cfg(target_arch = "wasm32")]
trait_supertrait_alias!(pub WasmNotSync: Any);
#[cfg(not(target_arch = "wasm32"))]
trait_supertrait_alias!(pub WasmNotSync: Any + Sync);
#[cfg(target_arch = "wasm32")]
trait_supertrait_alias!(pub WasmNotSendSync: Any);
#[cfg(not(target_arch = "wasm32"))]
trait_supertrait_alias!(pub WasmNotSendSync: Any + Send + Sync);

#[cfg(target_arch = "wasm32")]
pub type WasmVecAny = list_any::VecAny<dyn Any>;
#[cfg(not(target_arch = "wasm32"))]
pub type WasmVecAny = list_any::VecAny<dyn Any + Send + Sync>;

#[macro_export]
macro_rules! trait_supertrait_alias {
    ($vis:vis $name:ident: $($supertraits:tt)*) => {
        $vis trait $name: $($supertraits)* {}
        impl<T: $($supertraits)*> $name for T {}
    };
}
