use std::{num::NonZeroU32, sync::Arc};

use glam::UVec2;
use rend3_types::{TextureFormat, TextureUsages};
use wgpu::{
    Buffer, CommandBuffer, CommandEncoder, CommandEncoderDescriptor, Extent3d, Texture, TextureDescriptor,
    TextureDimension, TextureView, TextureViewDescriptor,
};

use crate::{
    resources::{CameraManager, TextureManagerReadyOutput},
    util::{
        output::OutputFrame,
        typedefs::{FastHashMap, FastHashSet, RendererStatistics, SsoString},
    },
    Renderer, INTERNAL_SHADOW_DEPTH_FORMAT,
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

pub struct RenderTarget {
    desc: RenderTargetDescriptor,
}

pub struct RenderGraph<'node> {
    targets: FastHashMap<SsoString, RenderTarget>,
    shadows: FastHashMap<usize, usize>,
    nodes: Vec<RenderGraphNode<'node>>,
}
impl<'node> RenderGraph<'node> {
    pub fn new() -> Self {
        Self {
            targets: FastHashMap::with_capacity_and_hasher(32, Default::default()),
            shadows: FastHashMap::with_capacity_and_hasher(32, Default::default()),
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
                    RenderResource::Shadow(..) => {
                        todo!()
                    }
                    RenderResource::Buffer(..) => {
                        todo!()
                    }
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
                    RenderResource::Shadow(..) => {
                        todo!()
                    }
                    RenderResource::Buffer(..) => {
                        todo!()
                    }
                    RenderResource::OutputTexture => continue,
                };
            }
        }

        let (shadow_mapping, shadow_views, shadow_array_view) =
            if let Some((size, coordinates)) = shadow_alloc::allocate_shadows(&self.shadows) {
                let shadow_texture = renderer.device.create_texture(&TextureDescriptor {
                    label: Some("shadow map"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: TextureDimension::D2,
                    format: INTERNAL_SHADOW_DEPTH_FORMAT,
                    usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                });

                let array_view = shadow_texture.create_view(&TextureViewDescriptor::default());

                let mut views = Vec::with_capacity(size.depth_or_array_layers as usize);
                for _ in 0..size.depth_or_array_layers {
                    views.push(shadow_texture.create_view(&TextureViewDescriptor {
                        base_array_layer: 0,
                        array_layer_count: Some(NonZeroU32::new(1).unwrap()),
                        ..TextureViewDescriptor::default()
                    }))
                }

                (coordinates, views, Some(array_view))
            } else {
                (FastHashMap::default(), Vec::new(), None)
            };

        let mut tagged_shadow_coords: Vec<_> = shadow_mapping.iter().collect();
        tagged_shadow_coords.sort_by_key(|(idx, _)| **idx);

        let shadow_coords = tagged_shadow_coords.into_iter().map(|(_, coords)| *coords).collect();

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
                texture_mapping: &active_views,
                shadow_mapping: &shadow_mapping,
                shadow_coord_array: &shadow_coords,
                shadow_array_view: &shadow_array_view,
                shadow_views: &shadow_views,
                output: output.as_view(),
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

pub struct ShadowTarget<'a> {
    pub view: &'a TextureView,
    pub offset: UVec2,
    pub size: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct ShadowCoordinates {
    pub layer: usize,
    pub offset: UVec2,
    pub size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum RenderResource {
    OutputTexture,
    Texture(SsoString),
    Shadow(usize),
    Buffer(usize),
}

pub struct RenderTargetHandle {
    resource: RenderResource,
}
pub struct ShadowTargetHandle(usize);

pub struct ShadowArrayHandle(());

pub struct RenderBufferHandle {
    resource: RenderResource,
}

pub struct RenderGraphTextureStore<'a> {
    texture_mapping: &'a FastHashMap<SsoString, TextureView>,
    shadow_mapping: &'a FastHashMap<usize, ShadowCoordinates>,
    shadow_coord_array: &'a Vec<ShadowCoordinates>,
    shadow_array_view: &'a Option<TextureView>,
    shadow_views: &'a Vec<TextureView>,
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
            .shadow_mapping
            .get(&handle.0)
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

    pub fn get_shadow_array(&self, _: ShadowArrayHandle) -> (&TextureView, &[ShadowCoordinates]) {
        let view = self
            .shadow_array_view
            .as_ref()
            .expect("internal rendergraph error: tried to get a shadow array when there is none");

        (view, &self.shadow_coord_array)
    }

    pub fn get_buffer(&self, handle: RenderBufferHandle) -> &Buffer {
        match handle.resource {
            RenderResource::Buffer(..) => {
                todo!()
            }
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
                &RenderGraphTextureStore<'_>,
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
        for (i, _size) in &self.graph.shadows {
            let resource = RenderResource::Shadow(*i);
            self.inputs.push(resource.clone());
        }
        ShadowArrayHandle(())
    }

    pub fn add_shadow_output(&mut self, index: usize, size: usize) -> RenderTargetHandle {
        let resource = RenderResource::Shadow(index);
        self.graph.shadows.insert(index, size);
        self.inputs.push(resource.clone());
        self.outputs.push(resource.clone());
        RenderTargetHandle { resource }
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
