use std::{
    any::Any,
    rc::{Rc},
    cell::{RefCell, UnsafeCell},
    collections::hash_map::Entry,
    marker::PhantomData,
    ops::Range,
    sync::Arc,
};

use glam::UVec2;
use wgpu::{
    Buffer, CommandBuffer, CommandEncoder, CommandEncoderDescriptor, LoadOp, Operations, RenderPass,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor, StoreOp, SurfaceTexture,
    Texture, TextureView, TextureViewDescriptor,
};

use super::ViewportRect;
use crate::{
    graph::{
        DataHandle, GraphResource, GraphSubResource, NodeExecutionContext, RenderGraphDataStore,
        RenderGraphEncoderOrPass, RenderGraphEncoderOrPassInner, RenderGraphNode, RenderGraphNodeBuilder,
        RenderPassTargets, RenderTargetDescriptor, RenderTargetHandle, RpassTemporaryPool, TextureRegion,
    },
    managers::{ShadowDesc, TextureManagerEvaluateOutput},
    util::typedefs::{FastHashMap, FastHashSet, RendererStatistics, SsoString},
    Renderer,
};

/// Result of evaluating all instructions.
pub struct InstructionEvaluationOutput {
    pub cmd_bufs: Vec<CommandBuffer>,
    pub d2_texture: TextureManagerEvaluateOutput,
    pub d2c_texture: TextureManagerEvaluateOutput,
    pub shadow_target_size: UVec2,
    pub shadows: Vec<ShadowDesc>,
    pub mesh_buffer: Arc<Buffer>,
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
        Self { inner: Box::new(RefCell::new(None::<T>)), dependencies: Vec::new() }
    }
}

#[derive(Debug, Copy, Clone)]
struct ResourceSpan {
    first_reference: usize,
    first_usage: Option<usize>,
    last_reference: Option<usize>,
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
            references: Vec::with_capacity(16),
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
                mip_start: 0,
                mip_end: desc.to_core().mip_count(),
                viewport: ViewportRect { offset: UVec2::ZERO, size: desc.resolution },
            }),
        };
        self.targets.push(desc);
        handle
    }

    pub fn add_imported_render_target(
        &mut self,
        texture: &'node dyn AsTextureReference,
        layers: Range<u32>,
        mips: Range<u8>,
        viewport: ViewportRect,
    ) -> RenderTargetHandle {
        let idx = self.imported_targets.len();
        self.imported_targets.push(texture);
        RenderTargetHandle {
            resource: GraphSubResource::ImportedTexture(TextureRegion {
                idx,
                layer_start: layers.start,
                layer_end: layers.end,
                mip_start: mips.start,
                mip_end: mips.end,
                viewport,
            }),
        }
    }

    pub fn add_data<T: 'static>(&mut self) -> DataHandle<T> {
        let idx = self.data.len();
        self.data.push(DataContents::new::<T>());
        DataHandle { idx, _phantom: PhantomData }
    }

    fn flatten_dependencies(data: &[DataContents], resource_list: &mut Vec<GraphSubResource>) {
        let mut idx = 0;
        // We use a while loop so we can walk the dependency tree recursively.
        while idx < resource_list.len() {
            if let GraphSubResource::Data(idx) = resource_list[idx] {
                resource_list.extend_from_slice(&data[idx].dependencies);

                // We can fall victim to cycles with this, so we assert on the length not being redonkulously large.
                assert!(resource_list.len() < (1 << 20), "Rendergraph has dependencies of data that form a cycle");
            }
            idx += 1;
        }
    }

    pub fn execute(
        mut self,
        renderer: &'node Arc<Renderer>,
        eval_output: &'node mut InstructionEvaluationOutput,
    ) -> Option<RendererStatistics> {
        profiling::scope!("RenderGraph::execute");

        // Because data handles have dependencies, we flatten the inputs and outputs ahead of time to simplify things.
        // We do it in place to save a bunch of allocations.
        for node in &mut self.nodes {
            Self::flatten_dependencies(&self.data, &mut node.inputs);
            Self::flatten_dependencies(&self.data, &mut node.outputs);
            Self::flatten_dependencies(&self.data, &mut node.references);
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
                    awaiting_inputs.extend(node.inputs.iter().map(|i| i.to_resource()));
                    // Push our node on the new list
                    pruned_node_list.push(node)
                }
            }
            // We iterated backwards to prune nodes, so flip it back to normal.
            pruned_node_list.reverse();
        }

        let mut resource_spans = FastHashMap::<_, ResourceSpan>::default();
        {
            profiling::scope!("Resource Span Analysis");
            // Iterate through all the nodes, tracking the index where they are first used,
            // and the index where they are last used.
            for (idx, node) in pruned_node_list.iter().enumerate() {
                // Add or update the range for all references
                for &reference in &node.references {
                    resource_spans
                        .entry(reference.to_resource())
                        .and_modify(|span| {
                            span.last_reference = Some(idx);
                        })
                        .or_insert(ResourceSpan { first_reference: idx, first_usage: None, last_reference: Some(idx) });
                }
                // Add or update the range for all inputs
                for &input in &node.inputs {
                    resource_spans
                        .entry(input.to_resource())
                        .and_modify(|span| {
                            span.first_usage.get_or_insert(idx);
                            span.last_reference = Some(idx)
                        })
                        .or_insert(ResourceSpan {
                            first_reference: idx,
                            first_usage: Some(idx),
                            last_reference: Some(idx),
                        });
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
                        .entry(output.to_resource())
                        .and_modify(|span| {
                            span.first_usage.get_or_insert(idx);
                            span.last_reference = end
                        })
                        .or_insert(ResourceSpan { first_reference: idx, first_usage: Some(idx), last_reference: end });
                }
            }
        }

        // For each node, record the list of textures whose references start and the list of
        // textures whose references end.
        let mut resource_changes = vec![(Vec::new(), Vec::new()); pruned_node_list.len()];
        {
            profiling::scope!("Compute Resource Span Deltas");
            for (&resource, span) in &resource_spans {
                resource_changes[span.first_reference].0.push(resource);
                if let Some(end) = span.last_reference {
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

        // Stores the Texture while a node is using it
        let mut active_textures = FastHashMap::default();
        {
            profiling::scope!("Render Target Allocation");
            for (starting, ending) in resource_changes {
                for start in starting {
                    match start {
                        GraphResource::Texture(idx) => {
                            let desc = &self.targets[idx];
                            let tex = graph_texture_store.get_texture(&renderer.device, desc.to_core());
                            // the whole texture is active
                            assert!(active_textures.insert(idx, tex).is_none());
                        }
                        GraphResource::Data(..) => {}
                        GraphResource::ImportedTexture(_) => {}
                        GraphResource::External => {}
                    };
                }

                for end in ending {
                    match end {
                        GraphResource::Texture(idx) => {
                            let tex = active_textures
                                .get(&idx)
                                .expect("internal rendergraph error: texture end with no start");

                            let desc = self.targets[idx].clone();
                            graph_texture_store.return_texture(desc.to_core(), Arc::clone(tex));
                        }
                        GraphResource::Data(..) => {}
                        GraphResource::ImportedTexture(_) => {}
                        GraphResource::External => {}
                    };
                }
            }
        }

        // Look through all touched resources, creating texture views for each region.
        let iter = pruned_node_list
            .iter()
            .flat_map(|node| [node.inputs.iter(), node.outputs.iter(), node.references.iter()])
            .flatten();

        // Map of region to texture view.
        let mut active_views = FastHashMap::default();
        // Map of region to imported texture view.
        let mut imported_views = FastHashMap::default();
        for sub_resource in iter {
            match *sub_resource {
                GraphSubResource::Texture(region) => {
                    if let Entry::Vacant(vacant) = active_views.entry(region) {
                        let view = active_textures[&region.idx].create_view(&TextureViewDescriptor {
                            base_array_layer: region.layer_start,
                            array_layer_count: Some(region.layer_end - region.layer_start),
                            base_mip_level: region.mip_start as u32,
                            mip_level_count: Some((region.mip_end - region.mip_start) as u32),
                            ..TextureViewDescriptor::default()
                        });
                        vacant.insert(view);
                    }
                }
                GraphSubResource::ImportedTexture(region) => {
                    if let Entry::Vacant(vacant) = imported_views.entry(region) {
                        let view =
                            self.imported_targets[region.idx].as_texture_ref().create_view(&TextureViewDescriptor {
                                base_array_layer: region.layer_start,
                                array_layer_count: Some(region.layer_end - region.layer_start),
                                base_mip_level: region.mip_start as u32,
                                mip_level_count: Some((region.mip_end - region.mip_start) as u32),
                                ..TextureViewDescriptor::default()
                            });
                        vacant.insert(view);
                    }
                }
                GraphSubResource::External => {}
                GraphSubResource::Data(_) => {}
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

        let encoder_cell =
            Rc::new(RefCell::new(renderer.device.create_command_encoder(&CommandEncoderDescriptor::default())));
        let rpass_temps_cell = UnsafeCell::new(RpassTemporaryPool::new());

        let mut next_rpass_idx = 0;
        let mut rpass = None;

        // Iterate through all the nodes and actually execute them.
        for (idx, node) in pruned_node_list.into_iter().enumerate() {
            if !compatible[idx] {
                // SAFETY: this drops the renderpass, letting us into everything it was
                // borrowing when we make the new renderpass.
                rpass = None;

                if let Some(ref desc) = node.rpass {
                    rpass = Some(Self::create_rpass_from_desc(
                        desc,
                        //  SAFETY: There are two things which borrow this encoder: the renderpass and the node's
                        //  encoder reference. Both of these have died by this point.
                        //  Now uses Rc, so that's checked.
                        &encoder_cell,
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
                };

                let mut encoder_or_rpass = match rpass {
                    Some(ref mut rpass) => {
                        let rpass_desc = node.rpass.unwrap();

                        let viewport = rpass_desc
                            .targets
                            .first()
                            .map_or_else(
                                || rpass_desc.depth_stencil.as_ref().unwrap().target.to_region(),
                                |t| t.color.to_region(),
                            )
                            .viewport;

                        rpass.borrow_mut().set_viewport(
                            viewport.offset.x as f32,
                            viewport.offset.y as f32,
                            viewport.size.x as f32,
                            viewport.size.y as f32,
                            0.0,
                            1.0,
                        );

                        RenderGraphEncoderOrPassInner::RenderPass(Rc::clone(rpass))
                    }
                    //  SAFETY: There is no active renderpass to borrow this. This reference lasts for the duration of
                    //  the call to exec.
                    //  Now uses Rc, so it's safe.
                    None => RenderGraphEncoderOrPassInner::Encoder(Rc::clone(&encoder_cell)),
                };

                profiling::scope!(&format!("Node: {}", node.label));

                let profiler_query = data_core.profiler.try_lock().unwrap().begin_query(
                    node.label,
                    &mut encoder_or_rpass,
                    &renderer.device,
                );

                let ctx = NodeExecutionContext {
                    renderer,
                    data_core,
                    encoder_or_pass: RenderGraphEncoderOrPass(encoder_or_rpass),
                    // SAFETY: This borrow, and all the objects allocated from it, lasts as long as the renderpass, and
                    // isn't used mutably until after the rpass dies
                    temps: unsafe { &*rpass_temps_cell.get() },
                    eval_output,
                    graph_data: store,
                    _phantom: PhantomData,
                };

                (node.exec)(ctx);

                let mut encoder_or_rpass = match rpass {
                    Some(ref mut rpass) => RenderGraphEncoderOrPassInner::RenderPass(Rc::clone(&rpass)),
                    //  SAFETY: There is no active renderpass to borrow this. This reference lasts for the duration of
                    //  the call to exec.
                    //  It's now safe code using Rc (JN)
                    None => RenderGraphEncoderOrPassInner::Encoder(Rc::clone(&encoder_cell)),
                };

                data_core.profiler.try_lock().unwrap().end_query(&mut encoder_or_rpass, profiler_query);
            }
        }

        // SAFETY: We drop the renderpass to make sure we can access both encoder_cell
        // and output_cell safely
        drop(rpass);

        // SAFETY: the renderpass has dropped, and so has all the uses of the data, and
        // the immutable borrows of the allocator.
        unsafe { (*rpass_temps_cell.get()).clear() }
        drop(rpass_temps_cell);

        //  SAFETY: this is safe as we've dropped all renderpasses that possibly borrowed it.
        //  It's now safe code using Rc. (JN)
        eval_output.cmd_bufs.push(Rc::into_inner(encoder_cell).unwrap().into_inner().finish());

        let mut resolve_encoder = renderer
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: Some("profile resolve encoder") });
        data_core.profiler.try_lock().unwrap().resolve_queries(&mut resolve_encoder);
        eval_output.cmd_bufs.push(resolve_encoder.finish());

        renderer.queue.submit(eval_output.cmd_bufs.drain(..));

        data_core.profiler.try_lock().unwrap().end_frame().unwrap();

        // This variable seems superfluous, but solves borrow checker issues with the borrow of data_core.
        let timers =
            data_core.profiler.try_lock().unwrap().process_finished_frame(renderer.queue.get_timestamp_period());

        timers
    }

    #[allow(clippy::too_many_arguments)]
    fn create_rpass_from_desc<'rpass>(
        desc: &RenderPassTargets,
        encoder: &Rc<RefCell<CommandEncoder>>,
        node_idx: usize,
        pass_end_idx: usize,
        resource_spans: &'rpass FastHashMap<GraphResource, ResourceSpan>,
        active_views: &'rpass FastHashMap<TextureRegion, TextureView>,
        active_imported_views: &'rpass FastHashMap<TextureRegion, TextureView>,
    ) -> Rc<RefCell<RenderPass<'static>>> {
        let color_attachments: Vec<_> = desc
            .targets
            .iter()
            .map(|target| {
                let view_span = resource_spans[&target.color.resource.to_resource()];

                let first_usage = view_span.first_usage.expect("internal rendergraph error: renderpass attachment counts as a usage, but no first usage registered on texture");

                let load = if first_usage == node_idx {
                    let clear_f64 = target.clear.as_dvec4();
                    LoadOp::Clear(wgpu::Color {
                        r: clear_f64.x,
                        g: clear_f64.y,
                        b: clear_f64.z,
                        a: clear_f64.w,
                    })
                } else {
                    LoadOp::Load
                };

                let store = if view_span.last_reference == Some(pass_end_idx) { StoreOp::Discard } else { StoreOp::Store };

                RenderPassColorAttachment {
                    view: match target.color.resource {
                        GraphSubResource::ImportedTexture(region) => &active_imported_views[&region],
                        GraphSubResource::Texture(region) => &active_views[&region],
                        _ => {
                            panic!("internal rendergraph error: using a non-texture as a renderpass attachment")
                        }
                    },
                    resolve_target: target.resolve.as_ref().map(|dep| match dep.resource {
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
            let resource = ds_target.target.resource;

            let view_span = resource_spans[&resource.to_resource()];

            let first_usage = view_span.first_usage.expect("internal rendergraph error: renderpass attachment counts as a usage, but no first usage registered on texture");

            let store = if view_span.last_reference == Some(pass_end_idx) { StoreOp::Discard } else { StoreOp::Store };

            let depth_ops = ds_target.depth_clear.map(|clear| {
                let load = if first_usage == node_idx {
                    LoadOp::Clear(clear)
                } else {
                    LoadOp::Load
                };

                Operations { load, store }
            });

            let stencil_load = ds_target.stencil_clear.map(|clear| {
                let load = if first_usage == node_idx {
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
        let mut binding = encoder.borrow_mut();
        let render_pass = binding.begin_render_pass(&RenderPassDescriptor {
            label: None,
            color_attachments: &color_attachments,
            depth_stencil_attachment,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        Rc::new(RefCell::new(render_pass.forget_lifetime()))  // return internally mutable form not tied to encoder lifetime
    }
}

impl<'node> Default for RenderGraph<'node> {
    fn default() -> Self {
        Self::new()
    }
}
