pub use default::*;
use fnv::FnvHashMap;
pub use passes::*;
pub use resources::*;

mod default;
mod passes;
mod resources;

pub struct RenderList {
    pub(crate) passes: Vec<RenderPass>,
    pub(crate) images: FnvHashMap<String, ImageResourceDescriptor>,
    pub(crate) buffers: FnvHashMap<String, BufferResourceDescriptor>,
}

impl RenderList {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            images: FnvHashMap::default(),
            buffers: FnvHashMap::default(),
        }
    }

    pub fn create_image(&mut self, name: impl ToString, image: ImageResourceDescriptor) {
        self.images.insert(name.to_string(), image);
    }

    pub fn create_buffer(&mut self, name: impl ToString, buffer: BufferResourceDescriptor) {
        self.buffers.insert(name.to_string(), buffer);
    }

    pub fn add_render_pass(&mut self, desc: RenderPassDescriptor) {
        self.passes.push(RenderPass { desc, ops: Vec::new() });
    }

    pub fn add_render_op(&mut self, desc: RenderOpDescriptor) {
        self.passes
            .last_mut()
            .expect("Added render op with no active render pass")
            .ops
            .push(desc);
    }
}

pub(crate) struct RenderPass {
    desc: RenderPassDescriptor,
    ops: Vec<RenderOpDescriptor>,
}
