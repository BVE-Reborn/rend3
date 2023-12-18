//! Material agnostic routine for forward rendering.
//!
//! Will default to the PBR shader code if custom code is not specified.

use std::{marker::PhantomData, sync::Arc};

use arrayvec::ArrayVec;
use encase::ShaderSize;
use rend3::{
    graph::{self, DataHandle, NodeResourceUsage, RenderGraph, RenderPassTargets},
    types::{GraphDataHandle, Material, SampleCount},
    util::{bind_merge::BindGroupBuilder, typedefs::FastHashMap},
    ProfileData, Renderer, RendererDataCore, RendererProfile, ShaderPreProcessor,
};
use serde::Serialize;
use wgpu::{
    BindGroup, BindGroupLayout, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState, DepthStencilState,
    FragmentState, IndexFormat, MultisampleState, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderModule, StencilState, TextureFormat,
    VertexState,
};

use crate::{
    common::{CameraSpecifier, PerMaterialArchetypeInterface, WholeFrameInterfaces},
    culling::{self, CullingBufferMap, DrawCall, DrawCallSet, InputOutputPartition},
};

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

enum DeclaredCullingOutput {
    Predicted,
    Residual(graph::DeclaredDependency<DataHandle<Arc<culling::DrawCallSet>>>),
}

impl DeclaredCullingOutput {
    /// Returns `true` if the declared culling output is [`Residual`].
    ///
    /// [`Residual`]: DeclaredCullingOutput::Residual
    #[must_use]
    fn is_residual(&self) -> bool {
        matches!(self, Self::Residual(..))
    }
}

pub enum CullingSource {
    /// We are rendering the first pass with the predicted triangles from last frame.
    ///
    /// This is used on the first pass.
    Predicted,
    /// We are rendering the second pass with the residual triangles from this frame.
    ///
    /// This is used when we are rendering the residual triangles from this frame, as part of the second pass.
    Residual(DataHandle<Arc<culling::DrawCallSet>>),
}

impl CullingSource {
    fn add_inner_data(&self, builder: &mut graph::RenderGraphNodeBuilder<'_, '_>) -> DeclaredCullingOutput {
        match self {
            CullingSource::Predicted => DeclaredCullingOutput::Predicted,
            CullingSource::Residual(handle) => {
                let handle = builder.add_data(*handle, NodeResourceUsage::Input);
                DeclaredCullingOutput::Residual(handle)
            }
        }
    }
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

    pub culling_buffer_map_handle: GraphDataHandle<CullingBufferMap>,

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
    pub culling_source: CullingSource,
    pub samples: SampleCount,
    pub renderpass: RenderPassTargets,
}

/// A set of pipelines for rendering a specific combination of a material.
pub struct ForwardRoutine<M: Material> {
    pipeline_s1: RenderPipeline,
    pipeline_s4: RenderPipeline,
    material_key: u64,
    culling_buffer_map_handle: GraphDataHandle<CullingBufferMap>,
    draw_call_set_cache_handle: GraphDataHandle<FastHashMap<CameraSpecifier, Arc<DrawCallSet>>>,
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
            draw_call_set_cache_handle: args.renderer.add_graph_data(FastHashMap::default()),
            culling_buffer_map_handle: args.culling_buffer_map_handle,
            _phantom: PhantomData,
        }
    }

    /// Add the given routine to the graph with the given settings.
    pub fn add_forward_to_graph<'node>(&'node self, args: ForwardRoutineArgs<'_, 'node, M>) {
        let mut builder = args.graph.add_node(args.label);

        builder.add_side_effect();

        let rpass_handle = builder.add_renderpass(args.renderpass.clone());

        let whole_frame_uniform_handle =
            builder.add_data(args.binding_data.whole_frame_uniform_bg, NodeResourceUsage::Input);
        let culling_output_handle = args.culling_source.add_inner_data(&mut builder);

        builder.build(move |mut ctx| {
            let rpass = ctx.encoder_or_pass.take_rpass(rpass_handle);
            let whole_frame_uniform_bg = ctx.graph_data.get_data(ctx.temps, whole_frame_uniform_handle).unwrap();

            // We need to store the draw call set in a cache so that next frame's predicted pass can use it.
            let mut draw_call_set_cache = ctx.data_core.graph_storage.get_mut(&self.draw_call_set_cache_handle);

            let draw_call_set = match culling_output_handle {
                // We are rendering the second pass with the residual triangles from this frame.
                DeclaredCullingOutput::Residual(handle) => {
                    // If there is no draw call set for this camera in the cache, there isn't actually anything to render.
                    let Some(draw_call_set) = ctx.graph_data.get_data(ctx.temps, handle) else {
                        return;
                    };

                    // As we're in the residual, we need to store the draw call set for the next frame.
                    draw_call_set_cache.insert(args.camera, Arc::clone(draw_call_set));

                    draw_call_set
                }
                // We are rendering the first pass with the predicted triangles from last frame.
                DeclaredCullingOutput::Predicted => {
                    // If there is no draw call set for this camera in the cache, that means we have yet to actually render anything,
                    // so either no objects yet exist, or we are in the first frame, so we can bail out.
                    let Some(draw_call_set) = draw_call_set_cache.get(&args.camera) else {
                        return;
                    };

                    draw_call_set
                }
            };
            let residual = culling_output_handle.is_residual() && args.camera.is_viewport();

            let culling_buffer_storage = ctx.data_core.graph_storage.get(&self.culling_buffer_map_handle);

            // If there are no culling buffers in storage yet, we are in the first frame. We depend on culling
            // to render anything, so just bail at this point.
            let Some(culling_buffers) = culling_buffer_storage.get_buffers(args.camera) else {
                return;
            };

            // We need to actually clone ownership of the underlying buffers and add them to renderpass temps,
            // so we can use them in the renderpass.
            let index_buffer = ctx.temps.add(Arc::clone(&culling_buffers.index_buffer));
            let draw_call_buffer = ctx.temps.add(Arc::clone(&culling_buffers.draw_call_buffer));

            // When we're rendering the residual data, we are post buffer flip. We want to be rendering using the
            // "input" partition, as this is the partition that all same-frame data is in.
            let partition = if residual {
                InputOutputPartition::Input
            } else {
                InputOutputPartition::Output
            };

            let per_material_bg = ctx.temps.add(
                BindGroupBuilder::new()
                    .append_buffer(ctx.data_core.object_manager.buffer::<M>().unwrap())
                    .append_buffer_with_size(
                        &draw_call_set.culling_data_buffer,
                        culling::ShaderBatchData::SHADER_SIZE.get(),
                    )
                    .append_buffer(&ctx.eval_output.mesh_buffer)
                    .append_buffer(&draw_call_set.per_camera_uniform)
                    .append_buffer(ctx.data_core.material_manager.archetype_view::<M>().buffer())
                    .build(
                        &ctx.renderer.device,
                        Some("Per-Material BG"),
                        &args.binding_data.per_material_bgl.bgl,
                    ),
            );

            let pipeline = match args.samples {
                SampleCount::One => &self.pipeline_s1,
                SampleCount::Four => &self.pipeline_s4,
            };
            rpass.set_index_buffer(
                index_buffer.slice(culling_buffers.index_buffer.partition_slice(partition)),
                IndexFormat::Uint32,
            );
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, whole_frame_uniform_bg, &[]);
            if let Some(v) = args.binding_data.extra_bgs {
                for (idx, bg) in v.iter().enumerate() {
                    rpass.set_bind_group((idx + 3) as _, bg, &[])
                }
            }
            if let ProfileData::Gpu(ref bg) = ctx.eval_output.d2_texture.bg {
                rpass.set_bind_group(2, bg, &[]);
            }

            // If there are no draw calls for this material, just bail.
            let Some(range) = draw_call_set.material_key_ranges.get(&self.material_key) else {
                return;
            };

            for (range_relative_idx, call) in draw_call_set.draw_calls[range.clone()].iter().enumerate() {
                // Help RA out
                let call: &DrawCall = call;
                // Add the base of the range to the index to get the actual index
                let idx = range_relative_idx + range.start;

                // If we're in cpu driven mode, we need to update the texture bind group.
                if ctx.renderer.profile.is_cpu_driven() {
                    rpass.set_bind_group(
                        2,
                        ctx.data_core.material_manager.texture_bind_group(call.bind_group_index),
                        &[],
                    );
                }
                rpass.set_bind_group(
                    1,
                    per_material_bg,
                    &[call.batch_index * culling::ShaderBatchData::SHADER_SIZE.get() as u32],
                );
                rpass.draw_indexed_indirect(
                    draw_call_buffer,
                    culling_buffers.draw_call_buffer.element_offset(partition, idx as u64),
                );
            }
        });
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
            entry_point: args.shaders.vs_entry,
            buffers: &[],
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
                RoutineType::Depth => DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
                RoutineType::Forward => DepthBiasState::default(),
            },
        }),
        multisample: MultisampleState {
            count: samples as u32,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: args.shaders.fs_module,
            entry_point: args.shaders.fs_entry,
            targets: &[],
        }),
        multiview: None,
    };
    if let Some(desc_callback) = args.descriptor_callback {
        desc_callback(&mut desc, &mut render_targets);
    }
    desc.fragment.as_mut().unwrap().targets = &render_targets;
    args.renderer.device.create_render_pipeline(&desc)
}
