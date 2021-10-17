use std::{any::Any, marker::PhantomData, sync::Arc};

use glam::UVec2;
use rend3_types::{BufferUsages, TextureFormat, TextureUsages};
use wgpu::{
    BindGroup, CommandBuffer, CommandEncoderDescriptor, Extent3d, Texture, TextureDescriptor, TextureDimension,
    TextureView, TextureViewDescriptor,
};

use crate::{
    resources::{CameraManager, ShadowCoordinates, TextureManagerReadyOutput},
    util::{
        output::OutputFrame,
        typedefs::{FastHashMap, FastHashSet, RendererStatistics, SsoString},
    },
    Renderer,
};

mod shadow_alloc;

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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct BufferTargetDescriptor {
    pub length: u64,
    pub usage: BufferUsages,
    pub mapped: bool,
}

pub struct RenderTarget {
    desc: RenderTargetDescriptor,
}

pub struct RenderGraph<'node> {
    targets: FastHashMap<SsoString, RenderTarget>,
    shadows: FastHashSet<usize>,
    data: FastHashMap<SsoString, Box<dyn Any>>, // Any is Option<T> where T is the stored data
    nodes: Vec<RenderGraphNode<'node>>,
}
impl<'node> RenderGraph<'node> {
    pub fn new() -> Self {
        Self {
            targets: FastHashMap::with_capacity_and_hasher(32, Default::default()),
            shadows: FastHashSet::with_capacity_and_hasher(32, Default::default()),
            data: FastHashMap::with_capacity_and_hasher(32, Default::default()),
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
        mut self,
        renderer: &Arc<Renderer>,
        mut output: OutputFrame,
        mut cmd_bufs: Vec<CommandBuffer>,
        ready_output: &ReadyData,
    ) -> Option<RendererStatistics> {
        let mut awaiting_inputs = FastHashSet::default();
        // The surface is used externally
        awaiting_inputs.insert(RenderResource::OutputTexture);

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

        let mut resource_spans = FastHashMap::<_, (usize, usize)>::default();
        // Iterate through all the nodes, tracking the index where they are first used, and the index where they are last used.
        for (idx, node) in pruned_node_list.iter().enumerate() {
            // Add or update the range for all inputs
            for input in &node.inputs {
                resource_spans
                    .entry(input.clone())
                    .and_modify(|range| range.1 = idx)
                    .or_insert((idx, idx));
            }
            // And the outputs
            for output in &node.outputs {
                resource_spans
                    .entry(output.clone())
                    .and_modify(|range| range.1 = idx)
                    .or_insert((idx, idx));
            }
        }

        // For each node, record the list of textures whose spans start and the list of textures whose spans end.
        let mut resource_changes = vec![(Vec::new(), Vec::new()); pruned_node_list.len()];
        for (texture, span) in resource_spans {
            resource_changes[span.0].0.push(texture.clone());
            resource_changes[span.1].1.push(texture);
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
        for (idx, (starting, ending)) in resource_changes.into_iter().enumerate() {
            for start in starting {
                match start {
                    RenderResource::Texture(name) => {
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
                    RenderResource::Shadow(..) => {}
                    RenderResource::Data(..) => {}
                    RenderResource::OutputTexture => {
                        acquire_idx = Some(idx);
                        continue;
                    }
                };
            }

            for end in ending {
                match end {
                    RenderResource::Texture(name) => {
                        let tex = active_textures
                            .remove(&name)
                            .expect("internal rendergraph error: texture end with no start");

                        let desc = self.targets[&name].desc;
                        textures.entry(desc).or_insert_with(|| Vec::with_capacity(16)).push(tex);
                    }
                    RenderResource::Shadow(..) => {}
                    RenderResource::Data(..) => {}
                    RenderResource::OutputTexture => continue,
                };
            }
        }

        let (prefix_cmd_buf_sender, prefix_cmd_buf_reciever) = flume::unbounded();
        let (cmd_buf_sender, cmd_buf_reciever) = flume::unbounded();

        let directional_light_manager = renderer.directional_light_manager.read();

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

            let mut store = RenderGraphTextureStore {
                texture_mapping: &active_views,
                shadow_array_view: directional_light_manager.get_bg(),
                shadow_coordinates: directional_light_manager.get_coords(),
                shadow_views: directional_light_manager.get_layer_views(),
                data: &mut self.data,
                output: output.as_view(),
            };
            (node.exec)(
                renderer,
                prefix_cmd_buf_sender.clone(),
                cmd_buf_sender.clone(),
                ready_output,
                &mut store,
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

pub struct ShadowTarget<'a> {
    pub view: &'a TextureView,
    pub offset: UVec2,
    pub size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RenderResource {
    OutputTexture,
    Texture(SsoString),
    Shadow(usize),
    Data(SsoString),
}

pub struct RenderTargetHandle {
    resource: RenderResource,
}
pub struct ShadowTargetHandle {
    idx: usize,
}

pub struct ShadowArrayHandle(());

pub struct DataHandle<T> {
    resource: RenderResource,
    _phantom: PhantomData<T>,
}

pub struct RenderGraphTextureStore<'a> {
    texture_mapping: &'a FastHashMap<SsoString, TextureView>,
    shadow_array_view: &'a BindGroup,
    shadow_coordinates: &'a [ShadowCoordinates],
    shadow_views: &'a [TextureView],
    data: &'a mut FastHashMap<SsoString, Box<dyn Any>>, // Any is Option<T> where T is the stored data
    output: Option<&'a TextureView>,
}

impl<'a> RenderGraphTextureStore<'a> {
    pub fn get_render_target(&self, handle: RenderTargetHandle) -> &TextureView {
        match handle.resource {
            RenderResource::Texture(name) => self
                .texture_mapping
                .get(&name)
                .expect("internal rendergraph error: failed to get named texture"),
            RenderResource::OutputTexture => self
                .output
                .expect("internal rendergraph error: tried to get unacquired surface image"),
            r => {
                panic!("internal rendergraph error: tried to get a {:?} as a render target", r)
            }
        }
    }

    pub fn get_shadow(&self, handle: ShadowTargetHandle) -> ShadowTarget<'_> {
        let coords = self
            .shadow_coordinates
            .get(handle.idx)
            .expect("internal rendergraph error: failed to get shadow mapping");
        ShadowTarget {
            view: self
                .shadow_views
                .get(coords.layer)
                .expect("internal rendergraph error: failed to get shadow layer"),
            offset: coords.offset,
            size: coords.size,
        }
    }

    pub fn get_shadow_array(&self, _: ShadowArrayHandle) -> &BindGroup {
        self.shadow_array_view
    }

    pub fn set_data<T: 'static>(&mut self, handle: DataHandle<T>, data: Option<T>) {
        match handle.resource {
            RenderResource::Data(name) => {
                *self
                    .data
                    .get_mut(&name)
                    .expect("internal rendergraph error: failed to get buffer")
                    .downcast_mut::<Option<T>>()
                    .expect("internal rendergraph error: downcasting failed") = data
            }
            r => {
                panic!("internal rendergraph error: tried to get a {:?} as a render target", r)
            }
        }
    }

    pub fn get_data<T: 'static>(&self, handle: DataHandle<T>) -> &Option<T> {
        match handle.resource {
            RenderResource::Data(name) => self
                .data
                .get(&name)
                .expect("internal rendergraph error: failed to get buffer")
                .downcast_ref::<Option<T>>()
                .expect("internal rendergraph error: downcasting failed"),
            r => {
                panic!("internal rendergraph error: tried to get a {:?} as a render target", r)
            }
        }
    }
}

#[allow(clippy::type_complexity)]
pub struct RenderGraphNode<'node> {
    inputs: Vec<RenderResource>,
    outputs: Vec<RenderResource>,
    exec: Box<
        dyn FnOnce(
                &Arc<Renderer>,
                flume::Sender<CommandBuffer>,
                flume::Sender<CommandBuffer>,
                &ReadyData,
                &mut RenderGraphTextureStore<'_>,
            ) + 'node,
    >,
}

pub struct RenderGraphNodeBuilder<'a, 'node> {
    graph: &'a mut RenderGraph<'node>,
    inputs: Vec<RenderResource>,
    outputs: Vec<RenderResource>,
}
impl<'a, 'node> RenderGraphNodeBuilder<'a, 'node> {
    pub fn add_render_target_input<S>(&mut self, name: S) -> RenderTargetHandle
    where
        SsoString: From<S>,
    {
        let resource = RenderResource::Texture(SsoString::from(name));
        self.inputs.push(resource.clone());
        RenderTargetHandle { resource }
    }

    pub fn add_render_target_output<S>(&mut self, name: S, desc: RenderTargetDescriptor) -> RenderTargetHandle
    where
        SsoString: From<S>,
    {
        let name = SsoString::from(name);
        self.graph.targets.entry(name.clone()).or_insert(RenderTarget { desc });
        let resource = RenderResource::Texture(name);
        self.inputs.push(resource.clone());
        self.outputs.push(resource.clone());
        RenderTargetHandle { resource }
    }

    pub fn add_surface_output(&mut self) -> RenderTargetHandle {
        let resource = RenderResource::OutputTexture;
        self.inputs.push(resource.clone());
        self.outputs.push(resource.clone());
        RenderTargetHandle { resource }
    }

    pub fn add_shadow_array_input(&mut self) -> ShadowArrayHandle {
        for i in &self.graph.shadows {
            let resource = RenderResource::Shadow(*i);
            self.inputs.push(resource.clone());
        }
        ShadowArrayHandle(())
    }

    pub fn add_shadow_output(&mut self, idx: usize) -> ShadowTargetHandle {
        let resource = RenderResource::Shadow(idx);
        self.graph.shadows.insert(idx);
        self.inputs.push(resource.clone());
        self.outputs.push(resource);
        ShadowTargetHandle { idx }
    }

    pub fn add_data_input<S, T>(&mut self, name: S) -> DataHandle<T>
    where
        SsoString: From<S>,
        T: 'static,
    {
        let name = SsoString::from(name);
        // TODO: error handling
        // TODO: move this validation to all types
        self.graph
            .data
            .get(&name)
            .expect("used input which has not been previously declared as a node's output")
            .downcast_ref::<Option<T>>()
            .expect("used custom data that was previously declared with a different type");
        let resource = RenderResource::Data(name);
        self.inputs.push(resource.clone());
        DataHandle {
            resource,
            _phantom: PhantomData,
        }
    }

    pub fn add_data_output<S, T>(&mut self, name: S) -> DataHandle<T>
    where
        SsoString: From<S>,
        T: 'static,
    {
        let name = SsoString::from(name);
        let resource = RenderResource::Data(name.clone());
        self.graph.data.insert(name, Box::new(None::<T>));
        self.inputs.push(resource.clone());
        self.outputs.push(resource.clone());
        DataHandle {
            resource,
            _phantom: PhantomData,
        }
    }

    pub fn build(
        self,
        exec: impl FnOnce(
                &Arc<Renderer>,
                flume::Sender<CommandBuffer>,
                flume::Sender<CommandBuffer>,
                &ReadyData,
                &mut RenderGraphTextureStore<'_>,
            ) + 'node,
    ) {
        self.graph.nodes.push(RenderGraphNode {
            inputs: self.inputs,
            outputs: self.outputs,
            exec: Box::new(exec),
        });
    }
}
