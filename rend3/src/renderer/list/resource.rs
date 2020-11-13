use crate::list::{BufferResourceDescriptor, ImageResourceDescriptor, ShaderSource};
use std::sync::Arc;
use wgpu::{Buffer, ShaderModule, Texture};

pub struct ImageResource {
    pub desc: ImageResourceDescriptor,
    pub image: Arc<Texture>,
}

pub struct BufferResource {
    pub desc: BufferResourceDescriptor,
    pub buffer: Arc<Buffer>,
}

pub struct ShaderResource {
    pub desc: ShaderSource,
    pub shader: Arc<ShaderModule>,
}
