use std::{
    any::Any,
    cell::{RefCell, UnsafeCell},
    marker::PhantomData,
    mem,
    sync::Arc,
};

use bumpalo::Bump;
use glam::UVec2;
use rend3_types::{BufferUsages, TextureFormat, TextureUsages};
use wgpu::{
    BindGroup, Color, CommandBuffer, CommandEncoder, CommandEncoderDescriptor, Extent3d, LoadOp, Operations,
    RenderPass, RenderPassColorAttachment, RenderPassDepthStencilAttachment, RenderPassDescriptor, Texture,
    TextureDescriptor, TextureDimension, TextureView, TextureViewDescriptor,
};
use wgpu_profiler::GpuProfiler;

use crate::{
    resources::{
        CameraManager, DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, ShadowCoordinates,
        TextureManager, TextureManagerReadyOutput,
    },
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

pub struct RenderGraph<'node> {
    targets: FastHashMap<SsoString, RenderTargetDescriptor>,
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
            passthrough: PassthroughDataContainer::none(),
            rpass: None,
        }
    }

    pub fn execute(
        self,
        renderer: &Arc<Renderer>,
        output: OutputFrame,
        mut cmd_bufs: Vec<CommandBuffer>,
        ready_output: &ReadyData,
    ) -> Option<RendererStatistics> {
        let mut awaiting_inputs = FastHashSet::default();
        // The surface is used externally
        awaiting_inputs.insert(GraphResource::OutputTexture);

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
        for (resource, span) in &resource_spans {
            resource_changes[span.0].0.push(resource.clone());
            resource_changes[span.1].1.push(resource.clone());
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
                    GraphResource::Texture(name) => {
                        let desc = self.targets[&name];
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
                    GraphResource::Shadow(..) => {}
                    GraphResource::Data(..) => {}
                    GraphResource::OutputTexture => {
                        acquire_idx = Some(idx);
                        continue;
                    }
                };
            }

            for end in ending {
                match end {
                    GraphResource::Texture(name) => {
                        let tex = active_textures
                            .remove(&name)
                            .expect("internal rendergraph error: texture end with no start");

                        let desc = self.targets[&name];
                        textures.entry(desc).or_insert_with(|| Vec::with_capacity(16)).push(tex);
                    }
                    GraphResource::Shadow(..) => {}
                    GraphResource::Data(..) => {}
                    GraphResource::OutputTexture => continue,
                };
            }
        }

        let mut profiler = renderer.profiler.lock();
        let camera_manager = renderer.camera_manager.read();
        let directional_light_manager = renderer.directional_light_manager.read();
        let material_manager = renderer.material_manager.read();
        let mesh_manager = renderer.mesh_manager.read();
        let object_manager = renderer.object_manager.read();
        let d2_texture_manager = renderer.d2_texture_manager.read();
        let d2c_texture_manager = renderer.d2c_texture_manager.read();

        let shadow_views = directional_light_manager.get_layer_views();

        let output_cell = UnsafeCell::new(output);
        let encoder_cell = UnsafeCell::new(
            renderer
                .device
                .create_command_encoder(&CommandEncoderDescriptor::default()),
        );
        let rpass_temps_cell = UnsafeCell::new(RpassTemporaryPool::new());

        let mut current_rpass_desc = None;
        let mut rpass = None;

        // Iterate through all the nodes and actually execute them.
        for (idx, node) in pruned_node_list.into_iter().enumerate() {
            if acquire_idx == Some(idx) {
                // SAFETY: this drops the renderpass, letting us into everything it was borrowing.
                rpass = None;
                current_rpass_desc = None;

                // SAFETY: the renderpass has died, so there are no outstanding immutible borrows of the structure, and all uses of the temporaries have died.
                unsafe { (&mut *rpass_temps_cell.get()).clear() };

                cmd_bufs.push(
                    mem::replace(
                        // SAFETY: There are two things which borrow this encoder: the renderpass and the node's encoder reference. Both of these have died by this point.
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

            if !RenderPassTargets::compatible(&current_rpass_desc, &node.rpass) {
                // SAFETY: this drops the renderpass, letting us into everything it was borrowing when we make the new renderpass.
                rpass = None;
                current_rpass_desc = node.rpass;

                if let Some(ref desc) = current_rpass_desc {
                    rpass = Some(Self::create_rpass_from_desc(
                        desc,
                        // SAFETY:  There are two things which borrow this encoder: the renderpass and the node's encoder reference. Both of these have died by this point.
                        unsafe { &mut *encoder_cell.get() },
                        idx,
                        // SAFETY: Same context as above.
                        unsafe { &mut *output_cell.get() },
                        &resource_spans,
                        &active_views,
                        shadow_views,
                    ));
                }
            }

            let store = RenderGraphDataStore {
                texture_mapping: &active_views,
                shadow_array_view: directional_light_manager.get_bg(),
                shadow_coordinates: directional_light_manager.get_coords(),
                shadow_views: directional_light_manager.get_layer_views(),
                data: &self.data,
                // SAFETY: This is only viewed mutably when no renderpass exists
                output: unsafe { &*output_cell.get() }.as_view(),

                profiler: &profiler,
                camera_manager: &camera_manager,
                directional_light_manager: &directional_light_manager,
                material_manager: &material_manager,
                mesh_manager: &mesh_manager,
                object_manager: &object_manager,
                d2_texture_manager: &d2_texture_manager,
                d2c_texture_manager: &d2c_texture_manager,
            };

            let encoder_or_rpass = match rpass {
                Some(ref mut rpass) => RenderGraphEncoderOrPassInner::RenderPass(rpass),
                // SAFETY: There is no active renderpass to borrow this. This reference lasts for the duration of the call to exec.
                None => RenderGraphEncoderOrPassInner::Encoder(unsafe { &mut *encoder_cell.get() }),
            };
            (node.exec)(
                node.passthrough,
                renderer,
                RenderGraphEncoderOrPass(encoder_or_rpass),
                // SAFETY: This borrow, and all the objects allocated from it, lasts as long as the renderpass, and isn't used mutably until after the rpass dies
                unsafe { &*rpass_temps_cell.get() },
                ready_output,
                store,
            );
        }

        // SAFETY: We drop the renderpass to make sure we can access both encoder_cell and output_cell safely
        drop(rpass);

        // SAFETY: the renderpass has dropped, and so has all the uses of the data, and the immutable borrows of the allocator.
        unsafe { (&mut *rpass_temps_cell.get()).clear() }
        drop(rpass_temps_cell);

        // SAFETY: this is safe as we've dropped all renderpasses that possibly borrowed it
        cmd_bufs.push(encoder_cell.into_inner().finish());

        let mut resolve_encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("profile resolve encoder"),
        });
        profiler.resolve_queries(&mut resolve_encoder);
        cmd_bufs.push(resolve_encoder.finish());

        renderer.queue.submit(cmd_bufs);

        // SAFETY: this is safe as we've dropped all renderpasses that possibly borrowed it
        output_cell.into_inner().present();

        profiler.end_frame().unwrap();
        profiler.process_finished_frame()
    }

    fn create_rpass_from_desc<'rpass>(
        desc: &RenderPassTargets,
        encoder: &'rpass mut CommandEncoder,
        pass_idx: usize,
        output: &'rpass OutputFrame,
        resource_spans: &'rpass FastHashMap<GraphResource, (usize, usize)>,
        active_views: &'rpass FastHashMap<SsoString, TextureView>,
        shadow_views: &'rpass [TextureView],
    ) -> RenderPass<'rpass> {
        let color_attachments: Vec<_> = desc
            .targets
            .iter()
            .map(|target| {
                let view_span = resource_spans[&target.target.resource];

                let load = if view_span.0 == pass_idx {
                    LoadOp::Clear(target.clear)
                } else {
                    LoadOp::Load
                };

                let store = view_span.1 != pass_idx;

                RenderPassColorAttachment {
                    view: match &target.target.resource {
                        GraphResource::OutputTexture => output
                            .as_view()
                            .expect("internal rendergraph error: tried to use output texture before acquire"),
                        GraphResource::Texture(t) => &active_views[t],
                        _ => {
                            panic!("internal rendergraph error: using a non-texture as a renderpass attachment")
                        }
                    },
                    resolve_target: target.resolve.as_ref().map(|handle| match &handle.resource {
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
        let depth_stencil_attachment = desc.depth_stencil.as_ref().map(|target| {
            let resource = match target.target {
                DepthHandle::RenderTarget(ref h) => h.resource.clone(),
                DepthHandle::Shadow(ref s) => GraphResource::Shadow(s.idx),
            };

            let view_span = resource_spans[&resource];

            let store = view_span.1 != pass_idx;

            let depth_ops = target.depth_clear.map(|clear| {
                let load = if view_span.0 == pass_idx {
                    LoadOp::Clear(clear)
                } else {
                    LoadOp::Load
                };

                Operations { load, store }
            });

            let stencil_load = target.stencil_clear.map(|clear| {
                let load = if view_span.0 == pass_idx {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum GraphResource {
    OutputTexture,
    Texture(SsoString),
    Shadow(usize),
    Data(SsoString),
}

#[derive(Debug, PartialEq)]
pub struct RenderTargetHandle {
    resource: GraphResource,
}

#[derive(Debug, PartialEq)]
pub struct ShadowTargetHandle {
    idx: usize,
}

pub struct ShadowArrayHandle;

pub struct DataHandle<T> {
    resource: GraphResource,
    _phantom: PhantomData<T>,
}

pub struct RenderGraphDataStore<'a> {
    texture_mapping: &'a FastHashMap<SsoString, TextureView>,
    shadow_array_view: &'a BindGroup,
    shadow_coordinates: &'a [ShadowCoordinates],
    shadow_views: &'a [TextureView],
    data: &'a FastHashMap<SsoString, Box<dyn Any>>, // Any is RefCell<Option<T>> where T is the stored data
    output: Option<&'a TextureView>,

    pub profiler: &'a GpuProfiler,
    pub camera_manager: &'a CameraManager,
    pub directional_light_manager: &'a DirectionalLightManager,
    pub material_manager: &'a MaterialManager,
    pub mesh_manager: &'a MeshManager,
    pub object_manager: &'a ObjectManager,
    pub d2_texture_manager: &'a TextureManager,
    pub d2c_texture_manager: &'a TextureManager,
}

impl<'a> RenderGraphDataStore<'a> {
    pub fn get_render_target(&self, handle: RenderTargetHandle) -> &'a TextureView {
        match handle.resource {
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

    pub fn get_shadow_array(&self, _: ShadowArrayHandle) -> &'a BindGroup {
        self.shadow_array_view
    }

    pub fn set_data<T: 'static>(&self, handle: DataHandle<T>, data: Option<T>) {
        match handle.resource {
            GraphResource::Data(name) => {
                *self
                    .data
                    .get(&name)
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

    pub fn get_data<T: 'static>(&self, temps: &'a RpassTemporaryPool<'a>, handle: DataHandle<T>) -> Option<&'a T> {
        match handle.resource {
            GraphResource::Data(name) => temps
                .add(
                    self.data
                        .get(&name)
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

pub struct PassthroughDataHandle<T> {
    _phantom: PhantomData<T>,
}

pub struct PassthroughDataContainer<'node> {
    data: Option<*const ()>,
    _phantom: PhantomData<&'node ()>,
}

impl<'node> PassthroughDataContainer<'node> {
    fn new<T>(data: &'node T) -> Self {
        Self {
            data: Some(data as *const T as *const ()),
            _phantom: PhantomData,
        }
    }

    fn none() -> Self {
        Self {
            data: None,
            _phantom: PhantomData,
        }
    }

    pub fn get<T>(self, _handle: PassthroughDataHandle<T>) -> &'node T {
        unsafe {
            &*(self
                .data
                .expect("internal rendergraph error: passthrough data handle corresponds to no passthrough data")
                as *const T)
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
    pub name: Option<SsoString>,
    pub targets: Vec<RenderPassTarget>,
    pub depth_stencil: Option<RenderPassDepthTarget>,
}

impl RenderPassTargets {
    fn compatible(this: &Option<Self>, other: &Option<Self>) -> bool {
        match (this, other) {
            (Some(this), Some(other)) => {
                let targets_compatible = this
                    .targets
                    .iter()
                    .zip(other.targets.iter())
                    .all(|(me, you)| me.target == you.target && me.resolve == you.resolve);

                let depth_compatible = this
                    .depth_stencil
                    .as_ref()
                    .zip(other.depth_stencil.as_ref())
                    .map_or(false, |(me, you)| me.target == you.target);

                targets_compatible && depth_compatible
            }
            (None, None) => true,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct RenderPassTarget {
    pub target: RenderTargetHandle,
    pub clear: Color,
    pub resolve: Option<RenderTargetHandle>,
}

#[derive(Debug, PartialEq)]
pub struct RenderPassDepthTarget {
    pub target: DepthHandle,
    pub depth_clear: Option<f32>,
    pub stencil_clear: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub enum DepthHandle {
    RenderTarget(RenderTargetHandle),
    Shadow(ShadowTargetHandle),
}

#[allow(clippy::type_complexity)]
pub struct RenderGraphNode<'node> {
    inputs: Vec<GraphResource>,
    outputs: Vec<GraphResource>,
    rpass: Option<RenderPassTargets>,
    passthrough: PassthroughDataContainer<'node>,
    exec: Box<
        dyn for<'b, 'pass> FnOnce(
                PassthroughDataContainer<'pass>,
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
    inputs: Vec<GraphResource>,
    outputs: Vec<GraphResource>,
    passthrough: PassthroughDataContainer<'node>,
    rpass: Option<RenderPassTargets>,
}
impl<'a, 'node> RenderGraphNodeBuilder<'a, 'node> {
    pub fn add_render_target_input<S>(&mut self, name: S) -> RenderTargetHandle
    where
        SsoString: From<S>,
    {
        let resource = GraphResource::Texture(SsoString::from(name));
        self.inputs.push(resource.clone());
        RenderTargetHandle { resource }
    }

    pub fn add_render_target_output<S>(&mut self, name: S, desc: RenderTargetDescriptor) -> RenderTargetHandle
    where
        SsoString: From<S>,
    {
        let name = SsoString::from(name);
        self.graph.targets.entry(name.clone()).or_insert(desc);
        let resource = GraphResource::Texture(name);
        self.inputs.push(resource.clone());
        self.outputs.push(resource.clone());
        RenderTargetHandle { resource }
    }

    pub fn add_surface_output(&mut self) -> RenderTargetHandle {
        let resource = GraphResource::OutputTexture;
        self.inputs.push(resource.clone());
        self.outputs.push(resource.clone());
        RenderTargetHandle { resource }
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
            self.inputs.push(resource.clone());
        }
        ShadowArrayHandle
    }

    pub fn add_shadow_output(&mut self, idx: usize) -> ShadowTargetHandle {
        let resource = GraphResource::Shadow(idx);
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
            .downcast_ref::<RefCell<Option<T>>>()
            .expect("used custom data that was previously declared with a different type");
        let resource = GraphResource::Data(name);
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
        let resource = GraphResource::Data(name.clone());
        self.graph.data.insert(name, Box::new(RefCell::new(None::<T>)));
        self.inputs.push(resource.clone());
        self.outputs.push(resource.clone());
        DataHandle {
            resource,
            _phantom: PhantomData,
        }
    }

    pub fn passthrough_data<T>(&mut self, data: &'node T) -> PassthroughDataHandle<T> {
        assert!(
            self.passthrough.data.is_none(),
            "Cannot have more than piece of passthrough data per node."
        );
        self.passthrough = PassthroughDataContainer::new(data);

        PassthroughDataHandle { _phantom: PhantomData }
    }

    pub fn build<F>(self, exec: F)
    where
        F: for<'b, 'pass> FnOnce(
                PassthroughDataContainer<'pass>,
                &Arc<Renderer>,
                RenderGraphEncoderOrPass<'b, 'pass>,
                &'pass RpassTemporaryPool<'pass>,
                &'pass ReadyData,
                RenderGraphDataStore<'pass>,
            ) + 'node,
    {
        self.graph.nodes.push(RenderGraphNode {
            inputs: self.inputs,
            outputs: self.outputs,
            rpass: self.rpass,
            passthrough: self.passthrough,
            exec: Box::new(exec),
        });
    }
}
