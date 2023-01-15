//! Material agnostic routine for forward rendering.
//!
//! Will default to the PBR shader code if custom code is not specified.

use std::marker::PhantomData;

use arrayvec::ArrayVec;
use encase::ShaderSize;
use rend3::{
    graph::{
        DataHandle, NodeResourceUsage, RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets,
        RenderTargetHandle,
    },
    types::{Handedness, Material, SampleCount},
    util::bind_merge::BindGroupBuilder,
    ProfileData, Renderer, RendererDataCore, RendererProfile, ShaderPreProcessor,
};
use serde::Serialize;
use wgpu::{
    BindGroup, BindGroupLayout, Color, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Face, FragmentState, FrontFace, IndexFormat, MultisampleState, PipelineLayoutDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderModule,
    StencilState, TextureFormat, VertexState,
};

use crate::{
    common::{PerMaterialArchetypeInterface, WholeFrameInterfaces},
    culling::{self, DrawCall},
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

pub struct RoutineArgs<'a, M> {
    pub name: &'a str,

    pub renderer: &'a Renderer,
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

pub struct RoutineAddToGraphArgs<'a, 'node, M> {
    pub graph: &'a mut RenderGraph<'node>,
    pub whole_frame_uniform_bg: DataHandle<BindGroup>,
    pub culled: DataHandle<culling::DrawCallSet>,
    pub per_material: &'node PerMaterialArchetypeInterface<M>,
    pub extra_bgs: Option<&'node [BindGroup]>,
    pub label: &'a str,
    pub samples: SampleCount,
    pub color: Option<RenderTargetHandle>,
    pub resolve: Option<RenderTargetHandle>,
    pub depth: RenderTargetHandle,
    /// Passed to the shader through the instance index.
    pub data: u32,
}

/// A set of pipelines for rendering a specific combination of a material.
pub struct ForwardRoutine<M: Material> {
    pub pipeline_s1: RenderPipeline,
    pub pipeline_s4: RenderPipeline,
    pub material_key: u64,
    pub _phantom: PhantomData<M>,
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
    pub fn new(args: RoutineArgs<'_, M>) -> Self {
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
    #[allow(clippy::too_many_arguments)]
    pub fn add_forward_to_graph<'node>(&'node self, args: RoutineAddToGraphArgs<'_, 'node, M>) {
        let mut builder = args.graph.add_node(args.label);

        let color_handle = builder.add_optional_render_target(args.color, NodeResourceUsage::InputOutput);
        let resolve_handle = builder.add_optional_render_target(args.resolve, NodeResourceUsage::InputOutput);
        let depth_handle = builder.add_render_target(args.depth, NodeResourceUsage::InputOutput);

        builder.add_side_effect();

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: match color_handle {
                Some(color) => vec![RenderPassTarget {
                    color,
                    clear: Color::BLACK,
                    resolve: resolve_handle,
                }],
                None => vec![],
            },
            depth_stencil: Some(RenderPassDepthTarget {
                target: depth_handle,
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        let whole_frame_uniform_handle = builder.add_data(args.whole_frame_uniform_bg, NodeResourceUsage::Input);
        let cull_handle = builder.add_data(args.culled, NodeResourceUsage::Input);

        builder.build(move |mut ctx| {
            let rpass = ctx.encoder_or_pass.take_rpass(rpass_handle);
            let whole_frame_uniform_bg = ctx.graph_data.get_data(ctx.temps, whole_frame_uniform_handle).unwrap();
            let culled = match ctx.graph_data.get_data(ctx.temps, cull_handle) {
                Some(c) => c,
                None => return,
            };

            let per_material_bg = ctx.temps.add(
                BindGroupBuilder::new()
                    .append_buffer(ctx.data_core.object_manager.buffer::<M>().unwrap())
                    .append_buffer_with_size(
                        &culled.buffers.object_reference,
                        culling::ShaderBatchData::SHADER_SIZE.get(),
                    )
                    .append_buffer(ctx.data_core.mesh_manager.buffer())
                    .append_buffer(ctx.data_core.material_manager.archetype_view::<M>().buffer())
                    .build(&ctx.renderer.device, Some("Per-Material BG"), &args.per_material.bgl),
            );

            let pipeline = match args.samples {
                SampleCount::One => &self.pipeline_s1,
                SampleCount::Four => &self.pipeline_s4,
            };

            rpass.set_index_buffer(culled.buffers.index.slice(..), IndexFormat::Uint32);
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, whole_frame_uniform_bg, &[]);
            if let Some(v) = args.extra_bgs {
                for (idx, bg) in v.iter().enumerate() {
                    rpass.set_bind_group((idx + 3) as _, bg, &[])
                }
            }
            if let ProfileData::Gpu(ref bg) = ctx.ready.d2_texture.bg {
                rpass.set_bind_group(2, bg, &[]);
            }

            let Some(range) = culled.material_key_ranges.get(&self.material_key) else {
                return;
            };
            for (idx, call) in culled.draw_calls[range.clone()].iter().enumerate() {
                let idx = idx + range.start;
                let call: &DrawCall = call;

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
                    &[idx as u32 * culling::ShaderBatchData::SHADER_SIZE.get() as u32],
                );
                rpass.draw_indexed(call.index_range.clone(), 0, args.data..args.data + 1);
            }
        });
    }
}

fn build_forward_pipeline_inner<M: Material>(
    pll: &wgpu::PipelineLayout,
    args: &RoutineArgs<'_, M>,
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
            front_face: match args.renderer.handedness {
                Handedness::Left => FrontFace::Cw,
                Handedness::Right => FrontFace::Ccw,
            },
            cull_mode: Some(Face::Back),
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
                RoutineType::Depth => DepthBiasState {
                    constant: -2,
                    slope_scale: -2.0,
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
