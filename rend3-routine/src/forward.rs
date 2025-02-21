//! Material agnostic routine for forward rendering.
//!
//! Will default to the PBR shader code if custom code is not specified.

use std::{cmp::Ordering, marker::PhantomData, sync::Arc};

use arrayvec::ArrayVec;
use encase::{ShaderSize, StorageBuffer};
use ordered_float::OrderedFloat;
use rend3::{
    graph::{DataHandle, NodeResourceUsage, RenderGraph, RenderPassTargets},
    managers::{CameraState, InternalObject, MaterialArchetypeView, TextureBindGroupIndex},
    types::{Material, RawObjectHandle, SampleCount, SortingOrder, SortingReason},
    util::bind_merge::BindGroupBuilder,
    ProfileData, Renderer, RendererDataCore, RendererProfile, ShaderPreProcessor,
};
use serde::Serialize;
use wgpu::{
    BindGroup, BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState,
    FragmentState, IndexFormat, MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderModule, StencilState,
    TextureFormat, VertexState,
};

use crate::common::{CameraSpecifier, PerMaterialArchetypeInterface, WholeFrameInterfaces};
use crate::uniforms::PerCameraUniform;

/// This structure is unused.
#[allow(dead_code)] // This structure is never constructed
#[derive(Serialize)]
struct ForwardPreprocessingArguments {
    profile: Option<RendererProfile>,
    vertex_array_counts: u32,
}

#[derive(Debug)]
pub enum RoutineType {
    Depth,
    Forward,
}

pub struct ShaderModulePair<'a> {
    pub vs_entry: &'a str,
    pub vs_module: &'a ShaderModule,
    pub fs_entry: &'a str,
    pub fs_module: &'a ShaderModule,
}

pub struct ForwardRoutineCreateArgs<'a, M> {
    pub name: &'a str,

    pub renderer: &'a Arc<Renderer>,
    pub data_core: &'a mut RendererDataCore,
    pub spp: &'a ShaderPreProcessor,

    pub interfaces: &'a WholeFrameInterfaces,
    pub per_material: &'a PerMaterialArchetypeInterface<M>,
    pub material_key: u64,

    pub routine_type: RoutineType,
    pub shaders: ShaderModulePair<'a>,

    pub extra_bgls: &'a [&'a BindGroupLayout],
    #[allow(clippy::type_complexity)]
    pub descriptor_callback: Option<&'a dyn Fn(&mut RenderPipelineDescriptor<'_>, &mut [Option<ColorTargetState>])>,
}

pub struct ForwardRoutineBindingData<'node, M> {
    /// Bind group holding references to all the uniforms needed by the entire frame.
    /// This is will be either the shadow pass uniforms, or the forward pass uniforms.
    ///
    /// This includes bindings provided by all the managers.
    pub whole_frame_uniform_bg: DataHandle<BindGroup>,
    /// Bind group layout for all the per-material uniforms for this material.
    ///
    /// The bind group is constructed in the rendergraph nodes.
    pub per_material_bgl: &'node PerMaterialArchetypeInterface<M>,
    /// Extra bind groups to be added to the pipeline.
    pub extra_bgs: Option<&'node [BindGroup]>,
}

pub struct ForwardRoutineArgs<'a, 'node, M> {
    pub graph: &'a mut RenderGraph<'node>,

    pub label: &'a str,

    pub camera: CameraSpecifier,
    pub binding_data: ForwardRoutineBindingData<'node, M>,

    /// Source of culling information, determines which triangles are rendered this pass.
    pub samples: SampleCount,
    pub renderpass: RenderPassTargets,
}

/// A set of pipelines for rendering a specific combination of a material.
pub struct ForwardRoutine<M: Material> {
    pipeline_s1: RenderPipeline,
    pipeline_s4: RenderPipeline,
    material_key: u64,
    _phantom: PhantomData<M>,
}
impl<M: Material> ForwardRoutine<M> {
    /// Create a new forward routine with optional customizations.
    ///
    /// Specifying vertex or fragment shaders will override the default ones.
    ///
    /// The order of BGLs passed to the shader is:  
    /// 0: Forward uniforms
    /// 1: Per material data  
    /// 2: Texture Array (GpuDriven) / Material (CpuDriven)  
    /// 3+: Contents of extra_bgls  
    ///
    /// Blend state is passed through to the pipeline.
    ///
    /// If use_prepass is true, depth tests/writes are set such that it is
    /// assumed a full depth-prepass has happened before.
    #[allow(clippy::too_many_arguments)]
    pub fn new(args: ForwardRoutineCreateArgs<'_, M>) -> Self {
        profiling::scope!("PrimaryPasses::new");

        let mut bgls: ArrayVec<&BindGroupLayout, 8> = ArrayVec::new();
        bgls.push(match args.routine_type {
            RoutineType::Depth => &args.interfaces.depth_uniform_bgl,
            RoutineType::Forward => &args.interfaces.forward_uniform_bgl,
        });
        bgls.push(&args.per_material.bgl);
        if args.renderer.profile == RendererProfile::GpuDriven {
            bgls.push(args.data_core.d2_texture_manager.gpu_bgl())
        } else {
            bgls.push(args.data_core.material_manager.get_bind_group_layout_cpu::<M>());
        }
        bgls.extend(args.extra_bgls.iter().copied());

        let pll = args.renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some(args.name),
            bind_group_layouts: &bgls,
            push_constant_ranges: &[],
        });

        Self {
            pipeline_s1: build_forward_pipeline_inner(&pll, &args, SampleCount::One),
            pipeline_s4: build_forward_pipeline_inner(&pll, &args, SampleCount::Four),
            material_key: args.material_key,
            _phantom: PhantomData,
        }
    }

    /// Add the given routine to the graph with the given settings.
    pub fn add_forward_to_graph<'node>(&'node self, args: ForwardRoutineArgs<'_, 'node, M>) {
        let mut builder = args.graph.add_node(args.label);

        let rpass_handle = builder.add_renderpass(args.renderpass.clone(), NodeResourceUsage::InputOutput);

        let whole_frame_uniform_handle =
            builder.add_data(args.binding_data.whole_frame_uniform_bg, NodeResourceUsage::Input);

        builder.build(move |mut ctx| {
            let rpass = ctx.encoder_or_pass.take_rpass(rpass_handle);
            let whole_frame_uniform_bg = ctx.graph_data.get_data(ctx.temps, whole_frame_uniform_handle).unwrap();

            let Some(objects) = ctx.data_core.object_manager.enumerated_objects::<M>() else {
                return;
            };

            let archetype_view = ctx.data_core.material_manager.archetype_view::<M>();

            let camera = match args.camera {
                CameraSpecifier::Viewport => &ctx.data_core.viewport_camera_state,
                CameraSpecifier::Shadow(idx) => &ctx.eval_output.shadows[idx as usize].camera,
            };

            let objects = sort(objects, archetype_view, self.material_key, camera);

            let per_camera_uniform_values = PerCameraUniform {
                view: camera.view(),
                view_proj: camera.view_proj(),
                frustum: camera.world_frustum(),
                object_count: objects.len() as u32,
            };

            let per_camera_uniform_buffer = ctx.temps.add(ctx.renderer.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Per Camera Uniform"),
                size: PerCameraUniform::SHADER_SIZE.get(),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: true,
            }));
            {   profiling::scope!("Mapping GPU memory");
                let mut mapping = per_camera_uniform_buffer.slice(..).get_mapped_range_mut();
                StorageBuffer::new(&mut *mapping).write(&per_camera_uniform_values).unwrap();
                drop(mapping);
                per_camera_uniform_buffer.unmap();
            }
            let per_material_bg = ctx.temps.add(
                BindGroupBuilder::new()
                    .append_buffer(ctx.data_core.object_manager.buffer::<M>().unwrap())
                    .append_buffer(&ctx.eval_output.mesh_buffer)
                    .append_buffer(per_camera_uniform_buffer)
                    .append_buffer(ctx.data_core.material_manager.archetype_view::<M>().buffer())
                    .build(&ctx.renderer.device, Some("Per-Material BG"), &args.binding_data.per_material_bgl.bgl),
            );

            let pipeline = match args.samples {
                SampleCount::One => &self.pipeline_s1,
                SampleCount::Four => &self.pipeline_s4,
            };
            let mut rpass = rpass.borrow_mut(); // actual rpass from Rc
            rpass.set_index_buffer(ctx.eval_output.mesh_buffer.slice(..), IndexFormat::Uint32);
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, whole_frame_uniform_bg, &[]);
            if let Some(v) = args.binding_data.extra_bgs {
                for (idx, bg) in v.iter().enumerate() {
                    rpass.set_bind_group((idx + 3) as _, bg, &[])
                }
            }
            if let ProfileData::Gpu(ref bg) = ctx.eval_output.d2_texture.bg {
                rpass.set_bind_group(2, Some(&**bg), &[]);
            }

            profiling::scope!("Binding/drawing");
            for (idx, object) in objects.into_iter() {
                let material = archetype_view.material(*object.material_handle);
                if material.inner.key() != self.material_key {
                    continue;
                }
                {   //////profiling::scope!("Binding"); //  Can't leave this enabled; major slowdown.
                    // If we're in cpu driven mode, we need to update the texture bind group.
                    if ctx.renderer.profile.is_cpu_driven() {
                        let texture_bind_group = material.bind_group_index.into_cpu();
                        rpass.set_bind_group(2, ctx.data_core.material_manager.texture_bind_group(texture_bind_group), &[]);
                    }
                    rpass.set_bind_group(1, Some(&*per_material_bg), &[]);
                }
                {   //////profiling::scope!("Drawing"); //  Can't leave this enabled; major slowdown.
                    rpass.draw_indexed(
                        object.inner.first_index..object.inner.first_index + object.inner.index_count,
                        0,
                        idx.idx as u32..idx.idx as u32 + 1,
                    )   
                }
            }
        });
    }
}

fn sort<'a, M, I>(
    objects: I,
    material_archetype: MaterialArchetypeView<'_, M>,
    requested_material_key: u64,
    camera: &CameraState,
) -> Vec<(RawObjectHandle, &'a InternalObject<M>)>
where
    M: Material,
    I: IntoIterator<Item = (RawObjectHandle, &'a InternalObject<M>)>,
    I::IntoIter: ExactSizeIterator,
{
    let objects = objects.into_iter();

    let mut sorted_objects = Vec::with_capacity(objects.len());
    {
        profiling::scope!("Sort Key Creation");
        for (raw_handle, object) in objects {
            let material = material_archetype.material(*object.material_handle);
            let object_material_key = material.inner.key();
            let sorting = material.inner.sorting();

            if object_material_key != requested_material_key {
                continue;
            }

            // Frustum culling
            if !camera.world_frustum().contains_sphere(object.inner.bounding_sphere) {
                continue;
            }

            let bind_group_index = material.bind_group_index.map_gpu(|_| TextureBindGroupIndex::DUMMY).into_common();

            let mut distance_sq = camera.location().distance_squared(object.location.into());

            if sorting.order == SortingOrder::BackToFront {
                distance_sq = -distance_sq;
            }
            sorted_objects.push((
                ObjectSortingKey {
                    bind_group_index,
                    distance: OrderedFloat(distance_sq),
                    sorting_reason: sorting.reason,
                },
                (raw_handle, object),
            ))
        }
    }

    {
        profiling::scope!("Sorting");
        sorted_objects.sort_unstable_by_key(|(k, _)| *k);
    }

    sorted_objects.into_iter().map(|(_, o)| o).collect()
}

#[derive(Debug, Clone, Copy, Eq)]
pub(super) struct ObjectSortingKey {
    pub bind_group_index: TextureBindGroupIndex,
    pub distance: OrderedFloat<f32>,
    pub sorting_reason: SortingReason,
}

impl PartialEq for ObjectSortingKey {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl PartialOrd for ObjectSortingKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ObjectSortingKey {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.sorting_reason.cmp(&other.sorting_reason) {
            Ordering::Equal => {}
            ord => return ord,
        }
        // The above comparison means that both sides are equal
        if self.sorting_reason == SortingReason::Requirement {
            match self.distance.cmp(&other.distance) {
                Ordering::Equal => {}
                ord => return ord,
            }
            self.bind_group_index.cmp(&other.bind_group_index)
        } else {
            match self.bind_group_index.cmp(&other.bind_group_index) {
                Ordering::Equal => {}
                ord => return ord,
            }
            self.distance.cmp(&other.distance)
        }
    }
}

fn build_forward_pipeline_inner<M: Material>(
    pll: &wgpu::PipelineLayout,
    args: &ForwardRoutineCreateArgs<'_, M>,
    samples: SampleCount,
) -> RenderPipeline {
    let mut render_targets: ArrayVec<_, 1> = ArrayVec::new();
    if matches!(args.routine_type, RoutineType::Forward) {
        render_targets.push(Some(ColorTargetState {
            format: TextureFormat::Rgba16Float,
            blend: None,
            write_mask: ColorWrites::all(),
        }));
    }
    let mut desc = RenderPipelineDescriptor {
        label: Some(args.name),
        layout: Some(pll),
        vertex: VertexState {
            module: args.shaders.vs_module,
            entry_point: Some(args.shaders.vs_entry),
            buffers: &[],
            compilation_options: PipelineCompilationOptions::default(), // use default WGPU options. New in WGPU 0.20 (JN)
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: args.renderer.handedness.into(),
            cull_mode: Some(match args.routine_type {
                RoutineType::Depth => wgpu::Face::Front,
                RoutineType::Forward => wgpu::Face::Back,
            }),
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState::default(),
            bias: match args.routine_type {
                // TODO: figure out what to put here
                RoutineType::Depth => DepthBiasState { constant: 0, slope_scale: 0.0, clamp: 0.0 },
                RoutineType::Forward => DepthBiasState::default(),
            },
        }),
        multisample: MultisampleState { count: samples as u32, ..Default::default() },
        fragment: Some(FragmentState {
            module: args.shaders.fs_module,
            entry_point: Some(args.shaders.fs_entry),
            targets: &[],
            compilation_options: PipelineCompilationOptions::default(), // use default WGPU options. New in WGPU 0.20 (JN)
        }),
        multiview: None,
        cache: None,    // (JN) no cache used
    };
    if let Some(desc_callback) = args.descriptor_callback {
        desc_callback(&mut desc, &mut render_targets);
    }
    desc.fragment.as_mut().unwrap().targets = &render_targets;
    args.renderer.device.create_render_pipeline(&desc)
}
