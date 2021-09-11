use std::sync::Arc;

use wgpu::CommandBuffer;

use crate::{util::output::OutputFrame, Renderer};

/// Routine which renders the current state of the renderer. The `rend3-pbr` crate offers a PBR, clustered-forward implementation of the render routine.
pub trait RenderRoutine: Send + Sync {
    fn render(&mut self, renderer: Arc<Renderer>, encoders: &mut Vec<CommandBuffer>, frame: &OutputFrame);
}
