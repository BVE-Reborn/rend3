use fnv::FnvHashMap;
pub use passes::*;
pub use resources::*;

mod passes;
mod resources;

pub(crate) struct RenderListResources {
    pub(crate) images: FnvHashMap<String, ImageResourceDescriptor>,
    pub(crate) buffers: FnvHashMap<String, BufferResourceDescriptor>,
}

pub struct RenderList {
    pub(crate) passes: Vec<RenderPass>,
    pub(crate) resources: RenderListResources,
}

impl RenderList {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            resources: RenderListResources {
                images: FnvHashMap::default(),
                buffers: FnvHashMap::default(),
            },
        }
    }

    pub fn create_image(&mut self, name: impl ToString, image: ImageResourceDescriptor) {
        self.resources.images.insert(name.to_string(), image);
    }

    pub fn create_buffer(&mut self, name: impl ToString, buffer: BufferResourceDescriptor) {
        self.resources.buffers.insert(name.to_string(), buffer);
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
impl Default for RenderList {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub(crate) struct RenderPass {
    pub desc: RenderPassDescriptor,
    pub ops: Vec<RenderOpDescriptor>,
}
