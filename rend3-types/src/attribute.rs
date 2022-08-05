use std::marker::PhantomData;

use bytemuck::Pod;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct VertexAttributeId(usize);

pub struct VertexAttribute<T> {
    name: &'static str,
    _phantom: PhantomData<T>,
}
impl<T> VertexAttribute<T> {
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            _phantom: PhantomData,
        }
    }

    pub fn name(&'static self) -> &'static str {
        self.name
    }

    pub fn id(&'static self) -> VertexAttributeId {
        VertexAttributeId(self as *const Self as usize)
    }
}

pub trait VertexFormat: Pod + Send + Sync + 'static {
    const SIZE: u32;
}

// TODO: More formats

impl VertexFormat for glam::Vec2 {
    const SIZE: u32 = 8;
}

impl VertexFormat for glam::Vec3 {
    const SIZE: u32 = 12;
}

impl VertexFormat for glam::Vec4 {
    const SIZE: u32 = 16;
}

pub static VERTEX_ATTRIBUTE_POSITION: VertexAttribute<glam::Vec3> = VertexAttribute::new("positions");
