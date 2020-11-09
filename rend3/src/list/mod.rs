pub use default::*;
pub use passes::*;
pub use resources::*;

mod default;
mod passes;
mod resources;

pub struct RenderList {
    pub(crate) sets: Vec<RenderPassSet>,
    pub(crate) images: Vec<ImageResourceDescriptor>,
    pub(crate) buffers: Vec<BufferResourceDescriptor>,
}

impl RenderList {
    pub fn new() -> Self {
        Self {
            sets: Vec::new(),
            images: Vec::new(),
            buffers: Vec::new(),
        }
    }

    pub fn start_render_pass_set(&mut self, desc: RenderPassSetDescriptor) {
        self.sets.push(RenderPassSet {
            run_rate: desc.run_rate,
            render_passes: Vec::new(),
        })
    }

    pub fn create_image(&mut self, image: ImageResourceDescriptor) {
        self.images.push(image);
    }

    pub fn create_buffer(&mut self, buffer: BufferResourceDescriptor) {
        self.buffers.push(buffer);
    }

    pub fn add_render_pass(&mut self, desc: RenderPassDescriptor) {
        self.sets
            .last_mut()
            .expect("Added render pass with no active render pass sets")
            .render_passes
            .push(RenderPass { desc, ops: Vec::new() });
    }

    pub fn add_render_op(&mut self, desc: RenderOpDescriptor) {
        self.sets
            .last_mut()
            .expect("Added render pass with no active render pass sets")
            .render_passes
            .last_mut()
            .expect("Added render op with no active render pass")
            .ops
            .push(desc);
    }
}

pub(crate) struct RenderPassSet {
    run_rate: RenderPassSetRunRate,
    render_passes: Vec<RenderPass>,
}

pub(crate) struct RenderPass {
    desc: RenderPassDescriptor,
    ops: Vec<RenderOpDescriptor>,
}
