use std::{
    any::Any,
    cell::{RefCell, UnsafeCell},
    marker::PhantomData,
    mem,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use bumpalo::Bump;
use glam::UVec2;
use rend3_types::{BufferUsages, SampleCount, TextureFormat, TextureUsages};
use wgpu::{
    Color, CommandBuffer, CommandEncoder, CommandEncoderDescriptor, LoadOp, Operations, RenderPass,
    RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor, TextureView,
    TextureViewDescriptor,
};
use wgpu_profiler::ProfilerCommandRecorder;

use crate::{
    managers::{
        CameraManager, DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, ShadowCoordinates,
        TextureManager, TextureManagerReadyOutput, SkeletonManager,
    },
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

#[derive(Debug, Clone)]
pub struct RenderTargetDescriptor {
    pub label: Option<SsoString>,
    pub resolution: UVec2,
    pub samples: SampleCount,
    pub format: TextureFormat,
    pub usage: TextureUsages,
}
impl RenderTargetDescriptor {
    fn to_core(&self) -> RenderTargetCore {
        RenderTargetCore {
            resolution: self.resolution,
            samples: self.samples,
            format: self.format,
            usage: self.usage,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RenderTargetCore {
    pub resolution: UVec2,
    pub samples: SampleCount,
    pub format: TextureFormat,
    pub usage: TextureUsages,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BufferTargetDescriptor {
    pub label: Option<SsoString>,
    pub length: u64,
    pub usage: BufferUsages,
    pub mapped: bool,
}

pub struct RenderGraph<'node> {
    targets: Vec<RenderTargetDescriptor>,
    shadows: FastHashSet<usize>,
    data: Vec<Box<dyn Any>>, // Any is RefCell<Option<T>> where T is the stored data
    nodes: Vec<RenderGraphNode<'node>>,
}
impl<'node> RenderGraph<'node> {
    pub fn new() -> Self {
        Self {
            targets: Vec::with_capacity(32),
            shadows: FastHashSet::with_capacity_and_hasher(32, Default::default()),
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
        self.targets.push(desc);
        RenderTargetHandle {
            resource: GraphResource::Texture(idx),
        }
    }

    pub fn add_surface_texture(&mut self) -> RenderTargetHandle {
        RenderTargetHandle {
            resource: GraphResource::OutputTexture,
        }
    }

    pub fn add_data<T: 'static>(&mut self) -> DataHandle<T> {
        let idx = self.data.len();
        self.data.push(Box::new(RefCell::new(None::<T>)));
        DataHandle {
            resource: GraphResource::Data(idx),
            _phantom: PhantomData,
        }
    }

    pub fn execute(
        self,
        renderer: &Arc<Renderer>,
        output: OutputFrame,
        mut cmd_bufs: Vec<CommandBuffer>,
        ready_output: &ReadyData,
    ) -> Option<RendererStatistics> {
        profiling::scope!("RenderGraph::execute");

        let mut awaiting_inputs = FastHashSet::default();
        // The surface is used externally
        awaiting_inputs.insert(GraphResource::OutputTexture);
        // External deps are used externally
        awaiting_inputs.insert(GraphResource::External);

        let mut pruned_node_list = Vec::with_capacity(self.nodes.len());
        {
            profiling::scope!("Dead Node Elimination");
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
                // And the outputs
                for &output in &node.outputs {
                    resource_spans
                        .entry(output)
                        .and_modify(|range| range.1 = Some(idx))
                        .or_insert((idx, Some(idx)));
                }
            }
        }

        // If the surface is used, we need treat it as if it has no end, as it will be
        // "used" after the graph is done.
        if let Some((_, surface_end)) = resource_spans.get_mut(&GraphResource::OutputTexture) {
            *surface_end = None;
        }

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
        // Which node index needs acquire to happen.
        let mut acquire_idx = None;
        {
            profiling::scope!("Render Target Allocation");
            for (idx, (starting, ending)) in resource_changes.into_iter().enumerate() {
                for start in starting {
                    match start {
                        GraphResource::Texture(idx) => {
                            let desc = &self.targets[idx];
                            let tex = graph_texture_store.get_texture(&renderer.device, desc.to_core());
                            let view = tex.create_view(&TextureViewDescriptor {
                                label: desc.label.as_deref(),
                                ..TextureViewDescriptor::default()
                            });
                            active_textures.insert(idx, tex);
                            active_views.insert(idx, view);
                        }
                        GraphResource::Shadow(..) => {}
                        GraphResource::Data(..) => {}
                        GraphResource::OutputTexture => {
                            acquire_idx = Some(idx);
                            continue;
                        }
                        GraphResource::External => {}
                    };
                }

                for end in ending {
                    match end {
                        GraphResource::Texture(idx) => {
                            let tex = active_textures
                                .remove(&idx)
                                .expect("internal rendergraph error: texture end with no start");

                            let desc = self.targets[idx].clone();
                            graph_texture_store.return_texture(desc.to_core(), tex);
                        }
                        GraphResource::Shadow(..) => {}
                        GraphResource::Data(..) => {}
                        GraphResource::OutputTexture => {}
                        GraphResource::External => {}
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

        let shadow_views = data_core.directional_light_manager.get_layer_views();

        let output_cell = UnsafeCell::new(output);
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
            if acquire_idx == Some(idx) {
                // SAFETY: this drops the renderpass, letting us into everything it was
                // borrowing.
                rpass = None;

                // SAFETY: the renderpass has died, so there are no outstanding immutible
                // borrows of the structure, and all uses of the temporaries have died.
                unsafe { (&mut *rpass_temps_cell.get()).clear() };

                cmd_bufs.push(
                    mem::replace(
                        // SAFETY: There are two things which borrow this encoder: the renderpass and the node's
                        // encoder reference. Both of these have died by this point.
                        unsafe { &mut *encoder_cell.get() },
                        renderer
                            .device
                            .create_command_encoder(&CommandEncoderDescriptor::default()),
                    )
                    .finish(),
                );

                // Early submit before acquire
                renderer.queue.submit(cmd_bufs.drain(..));

                // TODO: error
                // SAFETY: Same context as the above unsafe.
                unsafe { &mut *output_cell.get() }.acquire().unwrap();
            }

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
                        unsafe { &mut *output_cell.get() },
                        &resource_spans,
                        &active_views,
                        shadow_views,
                    ));
                }
                next_rpass_idx += 1;
            }

            {
                let store = RenderGraphDataStore {
                    texture_mapping: &active_views,
                    shadow_coordinates: data_core.directional_light_manager.get_coords(),
                    shadow_views: data_core.directional_light_manager.get_layer_views(),
                    data: &self.data,
                    // SAFETY: This is only viewed mutably when no renderpass exists
                    output: unsafe { &*output_cell.get() }.as_view(),

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
                    Some(ref mut rpass) => RenderGraphEncoderOrPassInner::RenderPass(rpass),
                    // SAFETY: There is no active renderpass to borrow this. This reference lasts for the duration of
                    // the call to exec.
                    None => RenderGraphEncoderOrPassInner::Encoder(unsafe { &mut *encoder_cell.get() }),
                };

                profiling::scope!(&node.label);

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
        unsafe { (&mut *rpass_temps_cell.get()).clear() }
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

        // SAFETY: this is safe as we've dropped all renderpasses that possibly borrowed
        // it
        output_cell.into_inner().present();

        data_core.profiler.end_frame().unwrap();
        data_core.profiler.process_finished_frame()
    }

    #[allow(clippy::too_many_arguments)]
    fn create_rpass_from_desc<'rpass>(
        desc: &RenderPassTargets,
        encoder: &'rpass mut CommandEncoder,
        node_idx: usize,
        pass_end_idx: usize,
        output: &'rpass OutputFrame,
        resource_spans: &'rpass FastHashMap<GraphResource, (usize, Option<usize>)>,
        active_views: &'rpass FastHashMap<usize, TextureView>,
        shadow_views: &'rpass [TextureView],
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
                    view: match &target.color.handle.resource {
                        GraphResource::OutputTexture => output
                            .as_view()
                            .expect("internal rendergraph error: tried to use output texture before acquire"),
                        GraphResource::Texture(t) => &active_views[t],
                        _ => {
                            panic!("internal rendergraph error: using a non-texture as a renderpass attachment")
                        }
                    },
                    resolve_target: target.resolve.as_ref().map(|dep| match &dep.handle.resource {
                        GraphResource::OutputTexture => output
                            .as_view()
                            .expect("internal rendergraph error: tried to use output texture before acquire"),
                        GraphResource::Texture(t) => &active_views[t],
                        _ => {
                            panic!("internal rendergraph error: using a non-texture as a renderpass attachment")
                        }
                    }),
                    ops: Operations { load, store },
                }
            })
            .collect();
        let depth_stencil_attachment = desc.depth_stencil.as_ref().map(|ds_target| {
            let resource = match ds_target.target {
                DepthHandle::RenderTarget(ref dep) => dep.handle.resource,
                DepthHandle::Shadow(ref s) => GraphResource::Shadow(s.idx),
            };

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
                view: match &resource {
                    GraphResource::OutputTexture => output
                        .as_view()
                        .expect("internal rendergraph error: tried to use output texture before acquire"),
                    GraphResource::Texture(t) => &active_views[t],
                    GraphResource::Shadow(s) => &shadow_views[*s],
                    _ => {
                        panic!("internal rendergraph error: using a non-texture as a renderpass attachment")
                    }
                },
                depth_ops,
                stencil_ops: stencil_load,
            }
        });

        // TODO: Properly read viewport
        // rpass.set_viewport(
        //     shadow_map.offset.x as f32,
        //     shadow_map.offset.y as f32,
        //     shadow_map.size as f32,
        //     shadow_map.size as f32,
        //     0.0,
        //     1.0,
        // );
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

pub struct ShadowTarget<'a> {
    pub view: &'a TextureView,
    pub offset: UVec2,
    pub size: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum GraphResource {
    OutputTexture,
    External,
    Texture(usize),
    Shadow(usize),
    Data(usize),
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct RenderTargetHandle {
    resource: GraphResource,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ShadowTargetHandle {
    idx: usize,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ShadowArrayHandle;

pub struct DataHandle<T> {
    resource: GraphResource,
    _phantom: PhantomData<T>,
}

impl<T> std::fmt::Debug for DataHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataHandle").field("resource", &self.resource).finish()
    }
}

impl<T> Copy for DataHandle<T> {}

impl<T> Clone for DataHandle<T> {
    fn clone(&self) -> Self {
        Self {
            resource: self.resource,
            _phantom: self._phantom,
        }
    }
}

impl<T> PartialEq for DataHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.resource == other.resource && self._phantom == other._phantom
    }
}

pub struct RenderGraphDataStore<'a> {
    texture_mapping: &'a FastHashMap<usize, TextureView>,
    shadow_coordinates: &'a [ShadowCoordinates],
    shadow_views: &'a [TextureView],
    data: &'a [Box<dyn Any>], // Any is RefCell<Option<T>> where T is the stored data
    output: Option<&'a TextureView>,

    pub camera_manager: &'a CameraManager,
    pub directional_light_manager: &'a DirectionalLightManager,
    pub material_manager: &'a MaterialManager,
    pub mesh_manager: &'a MeshManager,
    pub object_manager: &'a ObjectManager,
    pub skeleton_manager: &'a SkeletonManager,
    pub d2_texture_manager: &'a TextureManager,
    pub d2c_texture_manager: &'a TextureManager,
}

impl<'a> RenderGraphDataStore<'a> {
    pub fn get_render_target(&self, dep: DeclaredDependency<RenderTargetHandle>) -> &'a TextureView {
        match dep.handle.resource {
            GraphResource::Texture(name) => self
                .texture_mapping
                .get(&name)
                .expect("internal rendergraph error: failed to get named texture"),
            GraphResource::OutputTexture => self
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

    pub fn set_data<T: 'static>(&self, dep: DeclaredDependency<DataHandle<T>>, data: Option<T>) {
        match dep.handle.resource {
            GraphResource::Data(idx) => {
                *self
                    .data
                    .get(idx)
                    .expect("internal rendergraph error: failed to get buffer")
                    .downcast_ref::<RefCell<Option<T>>>()
                    .expect("internal rendergraph error: downcasting failed")
                    .try_borrow_mut()
                    .expect("tried to call set_data on a handle that has an outstanding borrow through get_data") = data
            }
            r => {
                panic!("internal rendergraph error: tried to get a {:?} as a render target", r)
            }
        }
    }

    pub fn get_data<T: 'static>(
        &self,
        temps: &'a RpassTemporaryPool<'a>,
        dep: DeclaredDependency<DataHandle<T>>,
    ) -> Option<&'a T> {
        match dep.handle.resource {
            GraphResource::Data(idx) => temps
                .add(
                    self.data
                        .get(idx)
                        .expect("internal rendergraph error: failed to get buffer")
                        .downcast_ref::<RefCell<Option<T>>>()
                        .expect("internal rendergraph error: downcasting failed")
                        .try_borrow()
                        .expect("internal rendergraph error: read-only borrow failed"),
                )
                .as_ref(),
            r => {
                panic!("internal rendergraph error: tried to get a {:?} as a render target", r)
            }
        }
    }
}

pub struct RenderPassHandle;

enum RenderGraphEncoderOrPassInner<'a, 'pass> {
    Encoder(&'a mut CommandEncoder),
    RenderPass(&'a mut RenderPass<'pass>),
}

impl<'a, 'pass> ProfilerCommandRecorder for RenderGraphEncoderOrPassInner<'a, 'pass> {
    fn write_timestamp(&mut self, query_set: &wgpu::QuerySet, query_index: u32) {
        match self {
            RenderGraphEncoderOrPassInner::Encoder(e) => e.write_timestamp(query_set, query_index),
            RenderGraphEncoderOrPassInner::RenderPass(rp) => rp.write_timestamp(query_set, query_index),
        }
    }

    fn push_debug_group(&mut self, label: &str) {
        match self {
            RenderGraphEncoderOrPassInner::Encoder(e) => e.push_debug_group(label),
            RenderGraphEncoderOrPassInner::RenderPass(rp) => rp.push_debug_group(label),
        }
    }

    fn pop_debug_group(&mut self) {
        match self {
            RenderGraphEncoderOrPassInner::Encoder(e) => e.pop_debug_group(),
            RenderGraphEncoderOrPassInner::RenderPass(rp) => rp.pop_debug_group(),
        }
    }
}

pub struct RenderGraphEncoderOrPass<'a, 'pass>(RenderGraphEncoderOrPassInner<'a, 'pass>);

impl<'a, 'pass> RenderGraphEncoderOrPass<'a, 'pass> {
    pub fn get_encoder(self) -> &'a mut CommandEncoder {
        match self.0 {
            RenderGraphEncoderOrPassInner::Encoder(e) => e,
            RenderGraphEncoderOrPassInner::RenderPass(_) => {
                panic!("called get_encoder when the rendergraph node asked for a renderpass");
            }
        }
    }

    pub fn get_rpass(self, _handle: RenderPassHandle) -> &'a mut RenderPass<'pass> {
        match self.0 {
            RenderGraphEncoderOrPassInner::Encoder(_) => {
                panic!("Internal rendergraph error: trying to get renderpass when one was not asked for")
            }
            RenderGraphEncoderOrPassInner::RenderPass(rpass) => rpass,
        }
    }
}

pub struct PassthroughDataRef<T> {
    node_id: usize,
    index: usize,
    _phantom: PhantomData<T>,
}

pub struct PassthroughDataRefMut<T> {
    node_id: usize,
    index: usize,
    _phantom: PhantomData<T>,
}

pub struct PassthroughDataContainer<'node> {
    node_id: usize,
    data: Vec<Option<*const ()>>,
    _phantom: PhantomData<&'node ()>,
}

impl<'node> PassthroughDataContainer<'node> {
    fn new() -> Self {
        static NODE_ID: AtomicUsize = AtomicUsize::new(0);
        Self {
            node_id: NODE_ID.fetch_add(1, Ordering::Relaxed),
            data: Vec::new(),
            _phantom: PhantomData,
        }
    }

    pub fn add_ref<T: 'node>(&mut self, data: &'node T) -> PassthroughDataRef<T> {
        let index = self.data.len();
        self.data.push(Some(<*const _>::cast(data)));
        PassthroughDataRef {
            node_id: self.node_id,
            index,
            _phantom: PhantomData,
        }
    }

    pub fn add_ref_mut<T: 'node>(&mut self, data: &'node mut T) -> PassthroughDataRefMut<T> {
        let index = self.data.len();
        self.data.push(Some(<*const _>::cast(data)));
        PassthroughDataRefMut {
            node_id: self.node_id,
            index,
            _phantom: PhantomData,
        }
    }

    pub fn get<T>(&mut self, handle: PassthroughDataRef<T>) -> &'node T {
        assert_eq!(
            handle.node_id, self.node_id,
            "Trying to use a passthrough data reference from another node"
        );
        unsafe {
            &*(self
                .data
                .get_mut(handle.index)
                .expect("internal rendergraph error: passthrough data handle corresponds to no passthrough data")
                .take()
                .expect("tried to retreve passthrough data more than once") as *const T)
        }
    }

    pub fn get_mut<T>(&mut self, handle: PassthroughDataRefMut<T>) -> &'node mut T {
        assert_eq!(
            handle.node_id, self.node_id,
            "Trying to use a passthrough data reference from another node"
        );
        unsafe {
            &mut *(self
                .data
                .get_mut(handle.index)
                .expect("internal rendergraph error: passthrough data handle corresponds to no passthrough data")
                .take()
                .expect("tried to retreve passthrough data more than once") as *const T as *mut T)
        }
    }
}

pub struct RpassTemporaryPool<'rpass> {
    bump: Bump,
    dtors: RefCell<Vec<Box<dyn FnOnce() + 'rpass>>>,
}
impl<'rpass> RpassTemporaryPool<'rpass> {
    fn new() -> Self {
        Self {
            bump: Bump::new(),
            dtors: RefCell::new(Vec::new()),
        }
    }

    #[allow(clippy::mut_from_ref)]
    pub fn add<T: 'rpass>(&'rpass self, v: T) -> &'rpass mut T {
        let r = self.bump.alloc(v);
        let ptr = r as *mut T;
        self.dtors
            .borrow_mut()
            .push(Box::new(move || unsafe { std::ptr::drop_in_place(ptr) }));
        r
    }

    unsafe fn clear(&mut self) {
        for dtor in self.dtors.get_mut().drain(..) {
            dtor()
        }
        self.bump.reset();
    }
}

#[derive(Debug, PartialEq)]
pub struct RenderPassTargets {
    pub targets: Vec<RenderPassTarget>,
    pub depth_stencil: Option<RenderPassDepthTarget>,
}

impl RenderPassTargets {
    fn compatible(this: Option<&Self>, other: Option<&Self>) -> bool {
        match (this, other) {
            (Some(this), Some(other)) => {
                let targets_compatible = this.targets.len() == other.targets.len()
                    && this
                        .targets
                        .iter()
                        .zip(other.targets.iter())
                        .all(|(me, you)| me.color == you.color && me.resolve == you.resolve);

                let depth_compatible = match (&this.depth_stencil, &other.depth_stencil) {
                    (Some(this_depth), Some(other_depth)) => this_depth == other_depth,
                    (None, None) => true,
                    _ => false,
                };

                targets_compatible && depth_compatible
            }
            (None, None) => true,
            _ => false,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct DeclaredDependency<Handle> {
    handle: Handle,
}

#[derive(Debug, PartialEq)]
pub struct RenderPassTarget {
    pub color: DeclaredDependency<RenderTargetHandle>,
    pub clear: Color,
    pub resolve: Option<DeclaredDependency<RenderTargetHandle>>,
}

#[derive(Debug, PartialEq)]
pub struct RenderPassDepthTarget {
    pub target: DepthHandle,
    pub depth_clear: Option<f32>,
    pub stencil_clear: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub enum DepthHandle {
    RenderTarget(DeclaredDependency<RenderTargetHandle>),
    Shadow(ShadowTargetHandle),
}

#[allow(clippy::type_complexity)]
pub struct RenderGraphNode<'node> {
    inputs: Vec<GraphResource>,
    outputs: Vec<GraphResource>,
    label: SsoString,
    rpass: Option<RenderPassTargets>,
    passthrough: PassthroughDataContainer<'node>,
    exec: Box<
        dyn for<'b, 'pass> FnOnce(
                &mut PassthroughDataContainer<'pass>,
                &Arc<Renderer>,
                RenderGraphEncoderOrPass<'b, 'pass>,
                &'pass RpassTemporaryPool<'pass>,
                &'pass ReadyData,
                RenderGraphDataStore<'pass>,
            ) + 'node,
    >,
}

pub struct RenderGraphNodeBuilder<'a, 'node> {
    graph: &'a mut RenderGraph<'node>,
    label: SsoString,
    inputs: Vec<GraphResource>,
    outputs: Vec<GraphResource>,
    passthrough: PassthroughDataContainer<'node>,
    rpass: Option<RenderPassTargets>,
}
impl<'a, 'node> RenderGraphNodeBuilder<'a, 'node> {
    pub fn add_render_target_input(&mut self, handle: RenderTargetHandle) -> DeclaredDependency<RenderTargetHandle> {
        self.inputs.push(handle.resource);
        DeclaredDependency { handle }
    }

    pub fn add_render_target_output(&mut self, handle: RenderTargetHandle) -> DeclaredDependency<RenderTargetHandle> {
        self.inputs.push(handle.resource);
        self.outputs.push(handle.resource);
        DeclaredDependency { handle }
    }

    pub fn add_optional_render_target_output(
        &mut self,
        handle: Option<RenderTargetHandle>,
    ) -> Option<DeclaredDependency<RenderTargetHandle>> {
        Some(self.add_render_target_output(handle?))
    }

    pub fn add_renderpass(&mut self, targets: RenderPassTargets) -> RenderPassHandle {
        assert!(
            self.rpass.is_none(),
            "Cannot have more than one graph-associated renderpass per node."
        );
        self.rpass = Some(targets);
        RenderPassHandle
    }

    pub fn add_shadow_array_input(&mut self) -> ShadowArrayHandle {
        for i in &self.graph.shadows {
            let resource = GraphResource::Shadow(*i);
            self.inputs.push(resource);
        }
        ShadowArrayHandle
    }

    pub fn add_shadow_output(&mut self, idx: usize) -> ShadowTargetHandle {
        let resource = GraphResource::Shadow(idx);
        self.graph.shadows.insert(idx);
        self.inputs.push(resource);
        self.outputs.push(resource);
        ShadowTargetHandle { idx }
    }

    pub fn add_data_input<T>(&mut self, handle: DataHandle<T>) -> DeclaredDependency<DataHandle<T>>
    where
        T: 'static,
    {
        self.add_data(handle, false)
    }

    pub fn add_data_output<T>(&mut self, handle: DataHandle<T>) -> DeclaredDependency<DataHandle<T>>
    where
        T: 'static,
    {
        self.add_data(handle, true)
    }

    fn add_data<T>(&mut self, handle: DataHandle<T>, output: bool) -> DeclaredDependency<DataHandle<T>>
    where
        T: 'static,
    {
        let idx = match handle.resource {
            GraphResource::Data(idx) => idx,
            _ => panic!("internal rendergraph error: tried to use a non-data resource as data"),
        };

        // TODO: error handling
        // TODO: move this validation to all types
        self.graph
            .data
            .get(idx)
            .expect("internal rendergraph error: cannot find data handle")
            .downcast_ref::<RefCell<Option<T>>>()
            .expect("used custom data that was previously declared with a different type");

        self.inputs.push(handle.resource);
        if output {
            self.outputs.push(handle.resource);
        }
        DeclaredDependency { handle }
    }

    pub fn add_external_output(&mut self) {
        self.inputs.push(GraphResource::External);
        self.outputs.push(GraphResource::External);
    }

    pub fn passthrough_ref<T: 'node>(&mut self, data: &'node T) -> PassthroughDataRef<T> {
        self.passthrough.add_ref(data)
    }

    pub fn passthrough_ref_mut<T: 'node>(&mut self, data: &'node mut T) -> PassthroughDataRefMut<T> {
        self.passthrough.add_ref_mut(data)
    }

    pub fn build<F>(self, exec: F)
    where
        F: for<'b, 'pass> FnOnce(
                &mut PassthroughDataContainer<'pass>,
                &Arc<Renderer>,
                RenderGraphEncoderOrPass<'b, 'pass>,
                &'pass RpassTemporaryPool<'pass>,
                &'pass ReadyData,
                RenderGraphDataStore<'pass>,
            ) + 'node,
    {
        self.graph.nodes.push(RenderGraphNode {
            label: self.label,
            inputs: self.inputs,
            outputs: self.outputs,
            rpass: self.rpass,
            passthrough: self.passthrough,
            exec: Box::new(exec),
        });
    }
}
