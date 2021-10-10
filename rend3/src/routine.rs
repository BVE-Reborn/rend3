use std::sync::Arc;

use glam::UVec2;
use rend3_types::TextureFormat;
use wgpu::{CommandBuffer, TextureView};

use crate::{
    resources::{CameraManager, TextureManagerReadyOutput},
    util::typedefs::{FastHashMap, FastHashSet, SsoString},
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

pub struct RenderTargetDescriptor {
    dim: UVec2,
    format: TextureFormat,
}

pub struct RenderTarget {
    desc: RenderTargetDescriptor,
}

pub struct RenderGraph {
    targets: FastHashMap<SsoString, RenderTarget>,
    nodes: Vec<RenderGraphNode>,
}
impl RenderGraph {
    pub fn new() -> Self {
        Self {
            targets: FastHashMap::with_capacity_and_hasher(32, Default::default()),
            nodes: Vec::with_capacity(64),
        }
    }

    pub fn add_node<'a>(&'a mut self) -> RenderGraphNodeBuilder<'a> {
        RenderGraphNodeBuilder {
            graph: self,
            node: RenderGraphNode {
                inputs: Vec::with_capacity(16),
                outputs: Vec::with_capacity(16),
            },
        }
    }

    pub fn build(&mut self) {
        let mut awaiting_inputs = FastHashSet::default();
        awaiting_inputs.insert(None);

        let mut pruned_node_list = Vec::with_capacity(self.nodes.len());
        // Iterate the nodes backwards to track dependencies
        for node in self.nodes.drain(..).rev() {
            // If any of our outputs are used by a previous node, we have reason to exist
            let outputs_used = node.outputs.iter().any(|o| awaiting_inputs.remove(&o));

            if outputs_used {
                // Add our inputs to be matched up with outputs.
                awaiting_inputs.extend(node.inputs.iter().cloned());
                // Push our node on the new list
                pruned_node_list.push(node)
            }
        }

        let mut texture_spans = FastHashMap::<_, (usize, usize)>::default();
        // Iterate through all the nodes, tracking the index where they are first used, and the index where they are last used.
        for (idx, node) in pruned_node_list.iter().enumerate() {
            // Add or update the range for all inputs
            for input in &node.inputs {
                texture_spans
                    .entry(input.clone())
                    .and_modify(|range| range.1 = idx)
                    .or_insert((idx, idx));
            }
            // And the outputs
            for output in &node.outputs {
                texture_spans
                    .entry(output.clone())
                    .and_modify(|range| range.1 = idx)
                    .or_insert((idx, idx));
            }
        }

        // For each node, record the list of textures whose spans start and the list of textures whose spans end.
        let mut texture_changes = vec![(Vec::new(), Vec::new()); pruned_node_list.len()];
        for (texture, span) in texture_spans {
            texture_changes[span.0].0.push(texture.clone());
            texture_changes[span.1].1.push(texture);
        }

        // Iterate through every node, allocating and deallocating textures as we go.
    }
}

pub struct RenderGraphNode {
    inputs: Vec<Option<SsoString>>,
    outputs: Vec<Option<SsoString>>,
}

pub struct RenderGraphNodeBuilder<'a> {
    graph: &'a mut RenderGraph,
    node: RenderGraphNode,
}
impl<'a> RenderGraphNodeBuilder<'a> {
    pub fn add_input(&mut self, name: SsoString, desc: RenderTargetDescriptor) {
        self.graph.targets.entry(name.clone()).or_insert(RenderTarget { desc });
        self.node.inputs.push(Some(name));
    }

    pub fn add_output(&mut self, name: SsoString, desc: RenderTargetDescriptor) {
        self.graph.targets.entry(name.clone()).or_insert(RenderTarget { desc });
        self.node.inputs.push(Some(name.clone()));
        self.node.outputs.push(Some(name));
    }

    pub fn add_surface_output(&mut self) {
        self.node.inputs.push(None);
        self.node.outputs.push(None);
    }

    pub fn build(self) {
        self.graph.nodes.push(self.node);
    }
}
