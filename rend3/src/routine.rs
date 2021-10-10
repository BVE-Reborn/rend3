use std::sync::Arc;

use glam::UVec2;
use rend3_types::TextureFormat;
use wgpu::{CommandBuffer, TextureView};

use crate::{
    resources::{CameraManager, TextureManagerReadyOutput},
    util::typedefs::{FastHashMap, SsoString},
    Renderer,
};

/// Output of calling ready on various managers.
pub struct ManagerReadyOutput {
    pub d2_texture: TextureManagerReadyOutput,
    pub d2c_texture: TextureManagerReadyOutput,
    pub directional_light_cameras: Vec<CameraManager>,
}

/// Routine which renders the current state of the renderer. The `rend3-pbr` crate offers a PBR, clustered-forward implementation of the render routine.
pub trait RenderRoutine<Input = (), Output = ()> {
    fn render(
        &mut self,
        renderer: Arc<Renderer>,
        cmd_bufs: flume::Sender<CommandBuffer>,
        ready: ManagerReadyOutput,
        input: Input,
        output: Output,
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RenderTargetDescriptor {
    dim: UVec2,
    format: TextureFormat,
}

pub struct RenderGraph {
    targets: Vec<(SsoString, RenderTargetDescriptor)>,
}
impl RenderGraph {
    pub fn new() -> Self {
        Self {
            targets: Vec::with_capacity(32),
        }
    }
}

pub struct RenderGraphNode {
    inputs: Vec<usize>,
    outputs: Vec<usize>
}

pub struct RenderGraphNodeBuilder<'a> {
    graph: &'a mut RenderGraph,
    node: RenderGraphNode,
}
impl<'a> RenderGraphNodeBuilder<'a> {
    pub fn add_input(&mut self, name: SsoString, descriptor: RenderTargetDescriptor) -> usize {
        let index = self.graph.targets.len();
        self.graph.targets.push((name, descriptor));
        self.node.inputs.push(index);
        index
    }

    pub fn add_output(&mut self, name: SsoString, descriptor: RenderTargetDescriptor) -> usize {
        let index = self.graph.targets.len();
        self.graph.targets.push((name, descriptor));
        self.node.inputs.push(index);
        self.node.outputs.push(index);
        index
    }

    pub fn 
}
