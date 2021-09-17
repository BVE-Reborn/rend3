use std::sync::Arc;

use wgpu::CommandBuffer;

use crate::{Renderer, resources::{CameraManager, InternalObject, TextureManagerReadyOutput}};

pub struct ManagerReadyOutput {
    pub objects: Vec<InternalObject>,
    pub d2_texture: TextureManagerReadyOutput,
    pub d2c_texture: TextureManagerReadyOutput,
    pub directional_light_cameras: Vec<CameraManager>
}

/// Routine which renders the current state of the renderer. The `rend3-pbr` crate offers a PBR, clustered-forward implementation of the render routine.
pub trait RenderRoutine<Input = (), Output = ()> {
    fn render(&mut self, renderer: Arc<Renderer>, cmd_bufs: flume::Sender<CommandBuffer>, ready: ManagerReadyOutput, input: Input, output: Output);
}
