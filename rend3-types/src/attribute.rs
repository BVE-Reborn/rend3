use std::{
    marker::PhantomData,
    ops::Deref,
    sync::atomic::{AtomicUsize, Ordering},
};

use bytemuck::Pod;
use once_cell::sync::Lazy;

#[derive(Debug, Copy, Clone)]
pub struct VertexAttributeId {
    inner: usize,
    metadata: &'static VertexFormatMetadata,
}

impl PartialEq for VertexAttributeId {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for VertexAttributeId {}

impl VertexAttributeId {
    pub fn metadata(&self) -> &'static VertexFormatMetadata {
        self.metadata
    }
}

static VERTEX_ATTRIBUTE_INDEX_ALLOCATOR: AtomicUsize = AtomicUsize::new(0);

pub struct VertexAttribute<T>
where
    T: VertexFormat,
{
    name: &'static str,
    id: Lazy<VertexAttributeId>,
    _phantom: PhantomData<T>,
}
impl<T> VertexAttribute<T>
where
    T: VertexFormat,
{
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            id: Lazy::new(|| VertexAttributeId {
                inner: VERTEX_ATTRIBUTE_INDEX_ALLOCATOR.fetch_add(1, Ordering::Relaxed),
                metadata: &T::METADATA,
            }),
            _phantom: PhantomData,
        }
    }

    pub fn name(&'static self) -> &'static str {
        self.name
    }

    pub fn id(&'static self) -> VertexAttributeId {
        *self.id
    }
}

impl<T> Deref for VertexAttribute<T>
where
    T: VertexFormat,
{
    type Target = VertexAttributeId;

    fn deref(&self) -> &Self::Target {
        &self.id
    }
}

#[derive(Debug)]
pub struct VertexFormatMetadata {
    pub size: u32,
    pub shader_extract_fn: &'static str,
    pub shader_type: &'static str,
}

pub trait VertexFormat: Pod + Send + Sync + 'static {
    const METADATA: VertexFormatMetadata;
}

// TODO: More formats

impl VertexFormat for glam::Vec2 {
    const METADATA: VertexFormatMetadata = VertexFormatMetadata {
        size: 8,
        shader_extract_fn: "extract_attribute_vec2_f32",
        shader_type: "vec2<f32>",
    };
}

impl VertexFormat for glam::Vec3 {
    const METADATA: VertexFormatMetadata = VertexFormatMetadata {
        size: 12,
        shader_extract_fn: "extract_attribute_vec3_f32",
        shader_type: "vec3<f32>",
    };
}

impl VertexFormat for glam::Vec4 {
    const METADATA: VertexFormatMetadata = VertexFormatMetadata {
        size: 16,
        shader_extract_fn: "extract_attribute_vec4_f32",
        shader_type: "vec4<f32>",
    };
}

impl VertexFormat for [u16; 4] {
    const METADATA: VertexFormatMetadata = VertexFormatMetadata {
        size: 8,
        shader_extract_fn: "extract_attribute_vec4_u16",
        shader_type: "vec4<u32>",
    };
}

impl VertexFormat for [u8; 4] {
    const METADATA: VertexFormatMetadata = VertexFormatMetadata {
        size: 8,
        shader_extract_fn: "extract_attribute_vec4_u8_unorm",
        shader_type: "vec4<f32>",
    };
}

pub static VERTEX_ATTRIBUTE_POSITION: VertexAttribute<glam::Vec3> = VertexAttribute::new("position");
pub static VERTEX_ATTRIBUTE_NORMAL: VertexAttribute<glam::Vec3> = VertexAttribute::new("normal");
pub static VERTEX_ATTRIBUTE_TANGENT: VertexAttribute<glam::Vec3> = VertexAttribute::new("tangent");
pub static VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_0: VertexAttribute<glam::Vec2> = VertexAttribute::new("texture_coords_0");
pub static VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_1: VertexAttribute<glam::Vec2> = VertexAttribute::new("texture_coords_1");
pub static VERTEX_ATTRIBUTE_COLOR_0: VertexAttribute<[u8; 4]> = VertexAttribute::new("color_0");
pub static VERTEX_ATTRIBUTE_COLOR_1: VertexAttribute<[u8; 4]> = VertexAttribute::new("color_1");
pub static VERTEX_ATTRIBUTE_JOINT_INDICES: VertexAttribute<[u16; 4]> = VertexAttribute::new("joint indices");
pub static VERTEX_ATTRIBUTE_JOINT_WEIGHTS: VertexAttribute<glam::Vec4> = VertexAttribute::new("joint weights");
