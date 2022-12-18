use std::{
    any::Any,
    cell::{RefCell, UnsafeCell},
    marker::PhantomData,
    num::NonZeroU32,
    ops::Range,
    sync::Arc,
};

use glam::UVec2;
use wgpu::{
    CommandBuffer, CommandEncoder, CommandEncoderDescriptor, LoadOp, Operations, RenderPass, RenderPassColorAttachment,
    RenderPassDepthStencilAttachment, RenderPassDescriptor, SurfaceTexture, Texture, TextureView,
    TextureViewDescriptor,
};

use crate::{
    graph::{
        DataHandle, GraphResource, GraphSubResource, PassthroughDataContainer, RenderGraphDataStore,
        RenderGraphEncoderOrPass, RenderGraphEncoderOrPassInner, RenderGraphNode, RenderGraphNodeBuilder,
        RenderPassTargets, RenderTargetDescriptor, RenderTargetHandle, RpassTemporaryPool, TextureRegion,
    },
    managers::{ShadowDesc, TextureManagerReadyOutput},
    util::typedefs::{FastHashMap, FastHashSet, RendererStatistics, SsoString},
    Renderer,
};

use super::ViewportRect;

/// Output of calling ready on various managers.
#[derive(Clone)]
pub struct ReadyData {
    pub d2_texture: TextureManagerReadyOutput,
    pub d2c_texture: TextureManagerReadyOutput,
    pub shadow_target_size: UVec2,
    pub shadows: Vec<ShadowDesc>,
}

pub trait AsTextureReference {
    fn as_texture_ref(&self) -> &Texture;
}

impl AsTextureReference for Texture {
    fn as_texture_ref(&self) -> &Texture {
        self
    }
}

impl AsTextureReference for SurfaceTexture {
    fn as_texture_ref(&self) -> &Texture {
        &self.texture
    }
}

pub(super) struct DataContents {
    // Any is RefCell<Option<T>> where T is the stored data
    pub(super) inner: Box<dyn Any>,
    pub(super) dependencies: Vec<GraphSubResource>,
}

impl DataContents {
    pub(super) fn new<T: 'static>() -> Self {
        Self {
            inner: Box::new(RefCell::new(None::<T>)),
            dependencies: Vec::new(),
        }
    }
}

/// Implementation of a rendergraph. See module docs for details.
pub struct RenderGraph<'node> {
    pub(super) targets: Vec<RenderTargetDescriptor>,
    pub(super) imported_targets: Vec<&'node dyn AsTextureReference>,
    pub(super) data: Vec<DataContents>,
    pub(super) nodes: Vec<RenderGraphNode<'node>>,
}
impl<'node> RenderGraph<'node> {
    pub fn new() -> Self {
        Self {
            targets: Vec::with_capacity(32),
            imported_targets: Vec::with_capacity(32),
            data: Vec::with_capacity(32),
            nodes: Vec::with_capacity(64),
        }
    }

    pub fn add_node<'a, S>(&'a mut self, label: S) -> RenderGraphNodeBuilder<'a, 'node>
    where
        SsoString: From<S>,
    {
        RenderGraphNodeBuilder {
            label: SsoString::from(label),
            graph: self,
            inputs: Vec::with_capacity(16),
            outputs: Vec::with_capacity(16),
            passthrough: PassthroughDataContainer::new(),
            rpass: None,
        }
    }

    pub fn add_render_target(&mut self, desc: RenderTargetDescriptor) -> RenderTargetHandle {
        let idx = self.targets.len();
        let handle = RenderTargetHandle {
            resource: GraphSubResource::Texture(TextureRegion {
                idx,
                layer_start: 0,
                layer_end: desc.depth,
                viewport: ViewportRect {
                    offset: UVec2::ZERO,
                    size: desc.resolution,
                },
            }),
        };
        self.targets.push(desc);
        handle
    }

    pub fn add_imported_render_target(
        &mut self,
        texture: &'node dyn AsTextureReference,
        layers: Range<u32>,
        viewport: ViewportRect,
    ) -> RenderTargetHandle {
        let idx = self.imported_targets.len();
        self.imported_targets.push(texture);
        RenderTargetHandle {
            resource: GraphSubResource::ImportedTexture(TextureRegion {
                idx,
                layer_start: layers.start,
                layer_end: layers.end,
                viewport,
            }),
        }
    }

    pub fn add_data<T: 'static>(&mut self) -> DataHandle<T> {
        let idx = self.data.len();
        self.data.push(DataContents::new::<T>());
        DataHandle {
            idx,
            _phantom: PhantomData,
        }
    }

    fn flatten_dependencies(data: &[DataContents], resource_list: &mut Vec<GraphSubResource>) {
        let mut idx = 0;
        // We use a while loop so we can walk the dependency tree recursively.
        while idx < resource_list.len() {
            if let GraphSubResource::Data(idx) = resource_list[idx] {
                resource_list.extend_from_slice(&data[idx].dependencies);

                // We can fall victim to cycles with this, so we assert on the length not being redonkulously large.
                assert!(
                    resource_list.len() < (1 << 20),
                    "Rendergraph has dependencies of data that form a cycle"
                );
            }
            idx += 1;
        }
    }

    pub fn execute(
        mut self,
        renderer: &Arc<Renderer>,
        mut cmd_bufs: Vec<CommandBuffer>,
        ready_output: &ReadyData,
    ) -> Option<RendererStatistics> {
        profiling::scope!("RenderGraph::execute");

        // Because data handles have dependencies, we flatten the inputs and outputs ahead of time to simplify things.
        // We do it in place to save a bunch of allocations.
        for node in &mut self.nodes {
            Self::flatten_dependencies(&self.data, &mut node.inputs);
            Self::flatten_dependencies(&self.data, &mut node.outputs);
        }

        let mut awaiting_inputs = FastHashSet::default();
        // Imported textures are always used
        for idx in 0..self.imported_targets.len() {
            awaiting_inputs.insert(GraphResource::ImportedTexture(idx));
        }
        // External deps are used externally
        awaiting_inputs.insert(GraphResource::External);

        let mut pruned_node_list = Vec::with_capacity(self.nodes.len());
        {
            profiling::scope!("Dead Node Elimination");
            // Iterate the nodes backwards to track dependencies
            for node in self.nodes.into_iter().rev() {
                // If any of our outputs are used by a previous node, we have reason to exist
                let outputs_used = node.outputs.iter().any(|o| awaiting_inputs.remove(&o.to_resource()));

                if outputs_used {
                    // Add our inputs to be matched up with outputs.
                    awaiting_inputs.extend(node.inputs.iter().map(GraphSubResource::to_resource));
                    // Push our node on the new list
                    pruned_node_list.push(node)
                }
            }
            // We iterated backwards to prune nodes, so flip it back to normal.
            pruned_node_list.reverse();
        }

        let mut resource_spans = FastHashMap::<_, (usize, Option<usize>)>::default();
        {
            profiling::scope!("Resource Span Analysis");
            // Iterate through all the nodes, tracking the index where they are first used,
            // and the index where they are last used.
            for (idx, node) in pruned_node_list.iter().enumerate() {
                // Add or update the range for all inputs
                for &input in &node.inputs {
                    resource_spans
                        .entry(input)
                        .and_modify(|range| range.1 = Some(idx))
                        .or_insert((idx, Some(idx)));
                }
                // All the outputs
                for &output in &node.outputs {
                    // All output textures we need treat them as if them has no end, as they will be
                    // "used" after the graph is done.
                    let end = match output {
                        GraphSubResource::ImportedTexture { .. } => None,
                        _ => Some(idx),
                    };
                    resource_spans
                        .entry(output)
                        .and_modify(|range| range.1 = end)
                        .or_insert((idx, end));
                }
            }
        }

        dbg!(&resource_spans);

        // For each node, record the list of textures whose spans start and the list of
        // textures whose spans end.
        let mut resource_changes = vec![(Vec::new(), Vec::new()); pruned_node_list.len()];
        {
            profiling::scope!("Compute Resource Span Deltas");
            for (&resource, span) in &resource_spans {
                resource_changes[span.0].0.push(resource);
                if let Some(end) = span.1 {
                    resource_changes[end].1.push(resource);
                }
            }
        }

        let mut data_core = renderer.data_core.lock();
        let data_core = &mut *data_core;

        // Iterate through every node, allocating and deallocating textures as we go.

        // Maps a texture description to any available textures. Will try to pull from
        // here instead of making a new texture.
        let graph_texture_store = &mut data_core.graph_texture_store;
        // Mark all textures as unused, so the ones that are unused can be culled after
        // this pass.
        graph_texture_store.mark_unused();

        // Stores the Texture while a texture is using it
        let mut active_textures = FastHashMap::default();
        // Maps a name to its actual texture view.
        let mut active_views = FastHashMap::default();
        // Views for imported textures
        let mut imported_views = FastHashMap::default();
        {
            profiling::scope!("Render Target Allocation");
            for (starting, ending) in resource_changes {
                for start in starting {
                    match start {
                        GraphSubResource::Texture(region) => {
                            let desc = &self.targets[region.idx];
                            let tex = graph_texture_store.get_texture(&renderer.device, desc.to_core());
                            let view = tex.create_view(&TextureViewDescriptor {
                                label: desc.label.as_deref(),
                                base_array_layer: region.layer_start,
                                array_layer_count: Some(
                                    NonZeroU32::new(region.layer_end - region.layer_start).unwrap(),
                                ),
                                ..TextureViewDescriptor::default()
                            });
                            // the whole texture is active
                            active_textures.insert(region.idx, tex);
                            // the view is for the region
                            active_views.insert(region, view);
                        }
                        GraphSubResource::Data(..) => {}
                        GraphSubResource::ImportedTexture(region) => {
                            // We don't actually care about start/end of imported views,
                            // we just need to make sure we create all the views needed.
                            imported_views.entry(region).or_insert_with(|| {
                                self.imported_targets[region.idx]
                                    .as_texture_ref()
                                    .create_view(&TextureViewDescriptor {
                                        base_array_layer: region.layer_start,
                                        array_layer_count: Some(
                                            NonZeroU32::new(region.layer_end - region.layer_start).unwrap(),
                                        ),
                                        ..TextureViewDescriptor::default()
                                    })
                            });
                        }
                        GraphSubResource::External => {}
                    };
                }

                for end in ending {
                    match end {
                        GraphSubResource::Texture(region) => {
                            let tex = active_textures
                                .remove(&region.idx)
                                .expect("internal rendergraph error: texture end with no start");

                            let desc = self.targets[region.idx].clone();
                            graph_texture_store.return_texture(desc.to_core(), tex);
                        }
                        GraphSubResource::Data(..) => {}
                        GraphSubResource::ImportedTexture(_) => {}
                        GraphSubResource::External => {}
                    };
                }
            }
        }

        // All textures that were ever returned are marked as used, so anything in here
        // that wasn't ever returned, was unused throughout the whole graph.
        graph_texture_store.remove_unused();

        // Iterate through all nodes and describe the node when they _end_
        let mut renderpass_ends = Vec::with_capacity(16);
        // If node is compatible with the previous node
        let mut compatible = Vec::with_capacity(pruned_node_list.len());
        {
            profiling::scope!("Renderpass Description");
            for (idx, node) in pruned_node_list.iter().enumerate() {
                // We always assume the first node is incompatible so the codepaths below are
                // consistent.
                let previous = match idx.checked_sub(1) {
                    Some(prev) => pruned_node_list[prev].rpass.as_ref(),
                    None => {
                        compatible.push(false);
                        continue;
                    }
                };

                compatible.push(RenderPassTargets::compatible(previous, node.rpass.as_ref()))
            }

            for (idx, &compatible) in compatible.iter().enumerate() {
                if compatible {
                    *renderpass_ends.last_mut().unwrap() = idx;
                } else {
                    renderpass_ends.push(idx)
                }
            }
        }

        profiling::scope!("Run Nodes");

        let encoder_cell = UnsafeCell::new(
            renderer
                .device
                .create_command_encoder(&CommandEncoderDescriptor::default()),
        );
        let rpass_temps_cell = UnsafeCell::new(RpassTemporaryPool::new());

        let mut next_rpass_idx = 0;
        let mut rpass = None;

        // Iterate through all the nodes and actually execute them.
        for (idx, mut node) in pruned_node_list.into_iter().enumerate() {
            println!(
                "{} {} {} {} {}",
                idx,
                &node.label,
                node.rpass.is_some(),
                compatible[idx],
                renderpass_ends[next_rpass_idx],
            );
            if !compatible[idx] {
                // SAFETY: this drops the renderpass, letting us into everything it was
                // borrowing when we make the new renderpass.
                rpass = None;

                if let Some(ref desc) = node.rpass {
                    rpass = Some(Self::create_rpass_from_desc(
                        desc,
                        // SAFETY: There are two things which borrow this encoder: the renderpass and the node's
                        // encoder reference. Both of these have died by this point.
                        unsafe { &mut *encoder_cell.get() },
                        idx,
                        renderpass_ends[next_rpass_idx],
                        // SAFETY: Same context as above.
                        &resource_spans,
                        &active_views,
                        &imported_views,
                    ));
                }
                next_rpass_idx += 1;
            }

            {
                let store = RenderGraphDataStore {
                    texture_mapping: &active_views,
                    external_texture_mapping: &imported_views,
                    data: &self.data,

                    camera_manager: &data_core.camera_manager,
                    directional_light_manager: &data_core.directional_light_manager,
                    material_manager: &data_core.material_manager,
                    mesh_manager: &data_core.mesh_manager,
                    skeleton_manager: &data_core.skeleton_manager,
                    object_manager: &data_core.object_manager,
                    d2_texture_manager: &data_core.d2_texture_manager,
                    d2c_texture_manager: &data_core.d2c_texture_manager,
                };

                let mut encoder_or_rpass = match rpass {
                    Some(ref mut rpass) => {
                        let rpass_desc = node.rpass.unwrap();

                        let viewport = rpass_desc
                            .targets
                            .first()
                            .map_or_else(
                                || rpass_desc.depth_stencil.as_ref().unwrap().target.handle.to_region(),
                                |t| t.color.handle.to_region(),
                            )
                            .viewport;

                        rpass.set_viewport(
                            viewport.offset.x as f32,
                            viewport.offset.y as f32,
                            viewport.size.x as f32,
                            viewport.size.y as f32,
                            0.0,
                            1.0,
                        );

                        RenderGraphEncoderOrPassInner::RenderPass(rpass)
                    }
                    // SAFETY: There is no active renderpass to borrow this. This reference lasts for the duration of
                    // the call to exec.
                    None => RenderGraphEncoderOrPassInner::Encoder(unsafe { &mut *encoder_cell.get() }),
                };

                profiling::scope!("Execute Node");

                data_core
                    .profiler
                    .begin_scope(&node.label, &mut encoder_or_rpass, &renderer.device);

                (node.exec)(
                    &mut node.passthrough,
                    renderer,
                    RenderGraphEncoderOrPass(encoder_or_rpass),
                    // SAFETY: This borrow, and all the objects allocated from it, lasts as long as the renderpass, and
                    // isn't used mutably until after the rpass dies
                    unsafe { &*rpass_temps_cell.get() },
                    ready_output,
                    store,
                );

                let mut encoder_or_rpass = match rpass {
                    Some(ref mut rpass) => RenderGraphEncoderOrPassInner::RenderPass(rpass),
                    // SAFETY: There is no active renderpass to borrow this. This reference lasts for the duration of
                    // the call to exec.
                    None => RenderGraphEncoderOrPassInner::Encoder(unsafe { &mut *encoder_cell.get() }),
                };

                data_core.profiler.end_scope(&mut encoder_or_rpass);
            }
        }

        // SAFETY: We drop the renderpass to make sure we can access both encoder_cell
        // and output_cell safely
        drop(rpass);

        // SAFETY: the renderpass has dropped, and so has all the uses of the data, and
        // the immutable borrows of the allocator.
        unsafe { (*rpass_temps_cell.get()).clear() }
        drop(rpass_temps_cell);

        // SAFETY: this is safe as we've dropped all renderpasses that possibly borrowed
        // it
        cmd_bufs.push(encoder_cell.into_inner().finish());

        let mut resolve_encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("profile resolve encoder"),
        });
        data_core.profiler.resolve_queries(&mut resolve_encoder);
        cmd_bufs.push(resolve_encoder.finish());

        renderer.queue.submit(cmd_bufs);

        data_core.profiler.end_frame().unwrap();
        data_core.profiler.process_finished_frame()
    }

    #[allow(clippy::too_many_arguments)]
    fn create_rpass_from_desc<'rpass>(
        desc: &RenderPassTargets,
        encoder: &'rpass mut CommandEncoder,
        node_idx: usize,
        pass_end_idx: usize,
        resource_spans: &'rpass FastHashMap<GraphSubResource, (usize, Option<usize>)>,
        active_views: &'rpass FastHashMap<TextureRegion, TextureView>,
        active_imported_views: &'rpass FastHashMap<TextureRegion, TextureView>,
    ) -> RenderPass<'rpass> {
        let color_attachments: Vec<_> = desc
            .targets
            .iter()
            .map(|target| {
                let view_span = resource_spans[&target.color.handle.resource];

                let load = if view_span.0 == node_idx {
                    LoadOp::Clear(target.clear)
                } else {
                    LoadOp::Load
                };

                let store = view_span.1 != Some(pass_end_idx);

                RenderPassColorAttachment {
                    view: match target.color.handle.resource {
                        GraphSubResource::ImportedTexture(region) => &active_imported_views[&region],
                        GraphSubResource::Texture(region) => &active_views[&region],
                        _ => {
                            panic!("internal rendergraph error: using a non-texture as a renderpass attachment")
                        }
                    },
                    resolve_target: target.resolve.as_ref().map(|dep| match dep.handle.resource {
                        GraphSubResource::ImportedTexture(region) => &active_imported_views[&region],
                        GraphSubResource::Texture(region) => &active_views[&region],
                        _ => {
                            panic!("internal rendergraph error: using a non-texture as a renderpass attachment")
                        }
                    }),
                    ops: Operations { load, store },
                }
            })
            .map(Option::Some)
            .collect();
        let depth_stencil_attachment = desc.depth_stencil.as_ref().map(|ds_target| {
            let resource = ds_target.target.handle.resource;

            let view_span = resource_spans[&resource];

            let store = view_span.1 != Some(pass_end_idx);

            let depth_ops = ds_target.depth_clear.map(|clear| {
                let load = if view_span.0 == node_idx {
                    LoadOp::Clear(clear)
                } else {
                    LoadOp::Load
                };

                Operations { load, store }
            });

            let stencil_load = ds_target.stencil_clear.map(|clear| {
                let load = if view_span.0 == node_idx {
                    LoadOp::Clear(clear)
                } else {
                    LoadOp::Load
                };

                Operations { load, store }
            });

            RenderPassDepthStencilAttachment {
                view: match resource {
                    GraphSubResource::ImportedTexture(region) => &active_imported_views[&region],
                    GraphSubResource::Texture(region) => &active_views[&region],
                    _ => {
                        panic!("internal rendergraph error: using a non-texture as a renderpass attachment")
                    }
                },
                depth_ops,
                stencil_ops: stencil_load,
            }
        });
        encoder.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &color_attachments,
            depth_stencil_attachment,
        })
    }
}

impl<'node> Default for RenderGraph<'node> {
    fn default() -> Self {
        Self::new()
    }
}
