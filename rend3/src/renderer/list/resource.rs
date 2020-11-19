use crate::list::{BufferResourceDescriptor, ImageResourceDescriptor};
use std::sync::Arc;
use wgpu::{Buffer, Texture, TextureView};

#[derive(Debug)]
pub struct ImageResource {
    pub desc: ImageResourceDescriptor,
    pub image: Arc<Texture>,
    pub image_view: Arc<TextureView>,
}

#[derive(Debug)]
pub struct BufferResource {
    pub desc: BufferResourceDescriptor,
    pub buffer: Arc<Buffer>,
}
