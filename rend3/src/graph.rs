use std::sync::Arc;

use glam::UVec2;
use rend3_types::{TextureFormat, TextureUsages};
use wgpu::{Buffer, CommandBuffer, CommandEncoder, CommandEncoderDescriptor, Extent3d, Texture, TextureDescriptor, TextureDimension, TextureView, TextureViewDescriptor};

use crate::{
    resources::{CameraManager, TextureManagerReadyOutput},
    util::{
        output::OutputFrame,
        typedefs::{FastHashMap, FastHashSet, RendererStatistics, SsoString},
    },
    Renderer,
};

/// Output of calling ready on various managers.
#[derive(Clone)]
pub struct ReadyData {
    pub d2_texture: TextureManagerReadyOutput,
    pub d2c_texture: TextureManagerReadyOutput,
    pub directional_light_cameras: Vec<CameraManager>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct RenderTargetDescriptor {
    pub dim: UVec2,
    pub format: TextureFormat,
    pub usage: TextureUsages,
}

pub struct RenderTarget {
    desc: RenderTargetDescriptor,
}

pub struct RenderGraph<'node> {
    targets: FastHashMap<SsoString, RenderTarget>,
    nodes: Vec<RenderGraphNode<'node>>,
}
impl<'node> RenderGraph<'node> {
    pub fn new() -> Self {
        Self {
            targets: FastHashMap::with_capacity_and_hasher(32, Default::default()),
            nodes: Vec::with_capacity(64),
        }
    }

    pub fn add_node<'a>(&'a mut self) -> RenderGraphNodeBuilder<'a, 'node> {
        RenderGraphNodeBuilder {
            graph: self,
            inputs: Vec::with_capacity(16),
            outputs: Vec::with_capacity(16),
        }
    }

    pub fn execute(
        self,
        renderer: &Arc<Renderer>,
        mut output: OutputFrame,
        mut cmd_bufs: Vec<CommandBuffer>,
        ready_output: &ReadyData,
    ) -> Option<RendererStatistics> {
        let mut awaiting_inputs = FastHashSet::default();
        awaiting_inputs.insert(None);

        let mut pruned_node_list = Vec::with_capacity(self.nodes.len());
        // Iterate the nodes backwards to track dependencies
        for node in self.nodes.into_iter().rev() {
            // If any of our outputs are used by a previous node, we have reason to exist
            let outputs_used = node.outputs.iter().any(|o| awaiting_inputs.remove(o));

            if outputs_used {
                // Add our inputs to be matched up with outputs.
                awaiting_inputs.extend(node.inputs.iter().cloned());
                // Push our node on the new list
                pruned_node_list.push(node)
            }
        }
        // We iterated backwards to prune nodes, so flip it back to normal.
        pruned_node_list.reverse();

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

        // Maps a texture description to any available textures. Will try to pull from here instead of making a new texture.
        let mut textures = FastHashMap::<RenderTargetDescriptor, Vec<Texture>>::default();
        // Stores the Texture while a texture is using it
        let mut active_textures = FastHashMap::default();
        // Maps a name to its actual texture view.
        let mut active_views = FastHashMap::default();
        // Which node index needs acquire to happen.
        let mut acquire_idx = None;
        for (idx, (starting, ending)) in texture_changes.into_iter().enumerate() {
            for start in starting {
                let name = match start {
                    Some(name) => name,
                    None => {
                        acquire_idx = Some(idx);
                        continue;
                    }
                };
                let desc = self.targets[&name].desc;
                if let Some(tex) = textures.get_mut(&desc).and_then(Vec::pop) {
                    let view = tex.create_view(&TextureViewDescriptor {
                        label: Some(&name),
                        ..TextureViewDescriptor::default()
                    });
                    active_views.insert(name.clone(), view);
                } else {
                    let tex = renderer.device.create_texture(&TextureDescriptor {
                        label: None,
                        size: Extent3d {
                            width: desc.dim.x,
                            height: desc.dim.y,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        // TODO: multisampling
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: desc.format,
                        usage: desc.usage,
                    });
                    let view = tex.create_view(&TextureViewDescriptor {
                        label: Some(&name),
                        ..TextureViewDescriptor::default()
                    });
                    active_textures.insert(name.clone(), tex);
                    active_views.insert(name.clone(), view);
                }
            }

            for end in ending {
                let name = match end {
                    Some(name) => name,
                    None => continue,
                };

                let tex = active_textures
                    .remove(&name)
                    .expect("internal rendergraph error: texture end with no start");

                let desc = self.targets[&name].desc;
                textures.entry(desc).or_insert_with(|| Vec::with_capacity(16)).push(tex);
            }
        }

        let (prefix_cmd_buf_sender, prefix_cmd_buf_reciever) = flume::unbounded();
        let (cmd_buf_sender, cmd_buf_reciever) = flume::unbounded();

        // Iterate through all the nodes and actually execute them.
        for (idx, node) in pruned_node_list.into_iter().enumerate() {
            if acquire_idx == Some(idx) {
                while let Ok(buf) = prefix_cmd_buf_reciever.try_recv() {
                    cmd_bufs.push(buf);
                }
                while let Ok(buf) = cmd_buf_reciever.try_recv() {
                    cmd_bufs.push(buf);
                }

                // Early submit before acquire
                renderer.queue.submit(cmd_bufs.drain(..));

                // TODO: error
                output.acquire().unwrap();
            }

            let store = RenderGraphTextureStore {
                mapping: &active_views,
                surface: output.as_view(),
            };
            (node.exec)(
                renderer,
                prefix_cmd_buf_sender.clone(),
                cmd_buf_sender.clone(),
                ready_output,
                &store,
            );
        }

        while let Ok(buf) = prefix_cmd_buf_reciever.try_recv() {
            cmd_bufs.push(buf);
        }
        while let Ok(buf) = cmd_buf_reciever.try_recv() {
            cmd_bufs.push(buf);
        }

        let mut resolve_encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("profile resolve encoder"),
        });
        renderer.profiler.lock().resolve_queries(&mut resolve_encoder);
        cmd_bufs.push(resolve_encoder.finish());

        renderer.queue.submit(cmd_bufs);

        output.present();

        let mut profiler = renderer.profiler.lock();
        profiler.end_frame().unwrap();
        profiler.process_finished_frame()
    }
}

impl<'node> Default for RenderGraph<'node> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RenderTargetHandle {
    name: Option<SsoString>,
}

pub struct RenderGraphTextureStore<'a> {
    mapping: &'a FastHashMap<SsoString, TextureView>,
    surface: Option<&'a TextureView>,
}

impl<'a> RenderGraphTextureStore<'a> {
    pub fn get_render_target(&self, handle: RenderTargetHandle) -> &TextureView {
        if let Some(name) = handle.name {
            self.mapping
                .get(&name)
                .expect("internal rendergraph error: failed to get named texture")
        } else {
            self.surface
                .expect("internal rendergraph error: tried to get unacquired surface image")
        }
    }
}

#[allow(clippy::type_complexity)]
pub struct RenderGraphNode<'node> {
    inputs: Vec<Option<SsoString>>,
    outputs: Vec<Option<SsoString>>,
    exec: Box<
        dyn FnOnce(
                &Arc<Renderer>,
                flume::Sender<CommandBuffer>,
                flume::Sender<CommandBuffer>,
                &ManagerReadyOutput,
                &RenderGraphTextureStore<'_>,
            ) + 'node,
    >,
}

pub struct RenderGraphNodeBuilder<'a, 'node> {
    graph: &'a mut RenderGraph<'node>,
    inputs: Vec<Option<SsoString>>,
    outputs: Vec<Option<SsoString>>,
}
impl<'a, 'node> RenderGraphNodeBuilder<'a, 'node> {
    pub fn add_render_target_input<S>(&mut self, name: S) -> RenderTargetHandle
    where
        SsoString: From<S>,
    {
        let name = SsoString::from(name);
        self.inputs.push(Some(name.clone()));
        RenderTargetHandle { name: Some(name) }
    }

    pub fn add_render_target_output<S>(&mut self, name: S, desc: RenderTargetDescriptor) -> RenderTargetHandle
    where
        SsoString: From<S>,
    {
        let name = SsoString::from(name);
        self.graph.targets.entry(name.clone()).or_insert(RenderTarget { desc });
        self.inputs.push(Some(name.clone()));
        self.outputs.push(Some(name.clone()));
        RenderTargetHandle { name: Some(name) }
    }

    pub fn add_surface_output(&mut self) -> RenderTargetHandle {
        self.inputs.push(None);
        self.outputs.push(None);
        RenderTargetHandle { name: None }
    }

    pub fn build(
        self,
        exec: impl FnOnce(
                &Arc<Renderer>,
                flume::Sender<CommandBuffer>,
                flume::Sender<CommandBuffer>,
                &ReadyData,
                &RenderGraphTextureStore<'_>,
            ) + 'node,
    ) {
        self.graph.nodes.push(RenderGraphNode {
            inputs: self.inputs,
            outputs: self.outputs,
            exec: Box::new(exec),
        });
    }
}
