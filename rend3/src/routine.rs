use std::sync::Arc;

use wgpu::CommandBuffer;

use crate::{util::output::OutputFrame, Renderer};

pub trait RenderRoutine: Send + Sync {
    fn render(&mut self, renderer: Arc<Renderer>, encoders: &mut Vec<CommandBuffer>, frame: &OutputFrame);
}
