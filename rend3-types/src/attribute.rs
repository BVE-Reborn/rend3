use std::{
    marker::PhantomData,
    ops::Deref,
    sync::atomic::{AtomicUsize, Ordering},
};

use bytemuck::Pod;
use once_cell::sync::OnceCell;

#[derive(Debug, Copy, Clone)]
pub struct VertexAttributeId {
    inner: usize,
    default_value: Option<&'static str>,
    name: &'static str,
    metadata: &'static VertexFormatMetadata,
}

impl PartialEq for VertexAttributeId {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}

impl Eq for VertexAttributeId {}

impl VertexAttributeId {
    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn metadata(&self) -> &'static VertexFormatMetadata {
        self.metadata
    }

    pub fn default_value(&self) -> Option<&'static str> {
        self.default_value
    }
}

static VERTEX_ATTRIBUTE_INDEX_ALLOCATOR: AtomicUsize = AtomicUsize::new(0);

pub struct VertexAttribute<T>
where
    T: VertexFormat,
{
    name: &'static str,
    default_value: Option<&'static str>,
    id: OnceCell<VertexAttributeId>,
    _phantom: PhantomData<T>,
}
impl<T> VertexAttribute<T>
where
    T: VertexFormat,
{
    pub const fn new(name: &'static str, default_value: Option<&'static str>) -> Self {
        Self {
            name,
            default_value,
            id: OnceCell::new(),
            _phantom: PhantomData,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn id(&self) -> &VertexAttributeId {
        self.id.get_or_init(|| VertexAttributeId {
            name: self.name,
            default_value: self.default_value,
            inner: VERTEX_ATTRIBUTE_INDEX_ALLOCATOR.fetch_add(1, Ordering::Relaxed),
            metadata: &T::METADATA,
        })
    }
}

impl<T> Deref for VertexAttribute<T>
where
    T: VertexFormat,
{
    type Target = VertexAttributeId;

    fn deref(&self) -> &Self::Target {
        self.id()
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

pub static VERTEX_ATTRIBUTE_POSITION: VertexAttribute<glam::Vec3> = VertexAttribute::new("position", None);
pub static VERTEX_ATTRIBUTE_NORMAL: VertexAttribute<glam::Vec3> = VertexAttribute::new("normal", None);
pub static VERTEX_ATTRIBUTE_TANGENT: VertexAttribute<glam::Vec3> = VertexAttribute::new("tangent", None);
pub static VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_0: VertexAttribute<glam::Vec2> =
    VertexAttribute::new("texture_coords_0", None);
pub static VERTEX_ATTRIBUTE_TEXTURE_COORDINATES_1: VertexAttribute<glam::Vec2> =
    VertexAttribute::new("texture_coords_1", None);
pub static VERTEX_ATTRIBUTE_COLOR_0: VertexAttribute<[u8; 4]> = VertexAttribute::new("color_0", Some("vec4<f32>(1.0)"));
pub static VERTEX_ATTRIBUTE_COLOR_1: VertexAttribute<[u8; 4]> = VertexAttribute::new("color_1", Some("vec4<f32>(1.0)"));
pub static VERTEX_ATTRIBUTE_JOINT_INDICES: VertexAttribute<[u16; 4]> = VertexAttribute::new("joint_indices", None);
pub static VERTEX_ATTRIBUTE_JOINT_WEIGHTS: VertexAttribute<glam::Vec4> = VertexAttribute::new("joint_weights", None);
