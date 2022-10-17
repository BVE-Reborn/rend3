//! Material agnostic routine for forward rendering.
//!
//! Will default to the PBR shader code if custom code is not specified.

use std::{borrow::Cow, marker::PhantomData};

use arrayvec::ArrayVec;
use encase::ShaderSize;
use rend3::{
    graph::{
        DataHandle, DepthHandle, RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets,
        RenderTargetHandle,
    },
    types::{Handedness, Material, MaterialArray, SampleCount, VertexAttributeId},
    util::bind_merge::BindGroupBuilder,
    ProfileData, Renderer, RendererDataCore, RendererProfile, ShaderPreProcessor,
};
use serde::Serialize;
use wgpu::{
    BindGroup, BindGroupLayout, BlendState, Color, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Face, FragmentState, FrontFace, IndexFormat, MultisampleState, PipelineLayoutDescriptor,
    PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderModule,
    ShaderModuleDescriptor, ShaderSource, StencilState, TextureFormat, VertexState,
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

/// A set of pipelines for rendering a specific combination of a material.
pub struct ForwardRoutine<M: Material> {
    pub pipeline_s1: RenderPipeline,
    pub pipeline_s4: RenderPipeline,
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
    pub fn new(
        renderer: &Renderer,
        data_core: &mut RendererDataCore,
        spp: &ShaderPreProcessor,
        interfaces: &WholeFrameInterfaces,
        per_material: &PerMaterialArchetypeInterface<M>,

        vertex: Option<(&str, &ShaderModule)>,
        fragment: Option<(&str, &ShaderModule)>,
        extra_bgls: &[BindGroupLayout],

        blend: Option<BlendState>,
        use_prepass: bool,
        primitive_topology: wgpu::PrimitiveTopology,
        label: &str,
    ) -> Self {
        profiling::scope!("PrimaryPasses::new");

        let sm_owned = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("forward"),
            source: ShaderSource::Wgsl(Cow::Owned(
                spp.render_shader(
                    "rend3-routine/opaque.wgsl",
                    &ForwardPreprocessingArguments {
                        profile: Some(renderer.profile),
                        vertex_array_counts: <M::SupportedAttributeArrayType as MaterialArray<
                            &'static VertexAttributeId,
                        >>::COUNT,
                    },
                )
                .unwrap(),
            )),
        });
        let forward_pass_vert;
        let vert_entry_point;
        match vertex {
            Some((inner_name, inner)) => {
                forward_pass_vert = inner;
                vert_entry_point = inner_name;
            }
            None => {
                vert_entry_point = "vs_main";
                forward_pass_vert = &sm_owned;
            }
        };

        let forward_pass_frag;
        let frag_entry_point;
        match fragment {
            Some((inner_name, inner)) => {
                forward_pass_frag = inner;
                frag_entry_point = inner_name;
            }
            None => {
                frag_entry_point = "fs_main";
                forward_pass_frag = &sm_owned;
            }
        };

        let mut bgls: ArrayVec<&BindGroupLayout, 8> = ArrayVec::new();
        bgls.push(&interfaces.forward_uniform_bgl);
        bgls.push(&per_material.bgl);
        if renderer.profile == RendererProfile::GpuDriven {
            bgls.push(data_core.d2_texture_manager.gpu_bgl())
        } else {
            bgls.push(data_core.material_manager.get_bind_group_layout_cpu::<M>());
        }
        bgls.extend(extra_bgls);

        let pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("opaque pass"),
            bind_group_layouts: &bgls,
            push_constant_ranges: &[],
        });

        let inner = |samples| {
            build_forward_pipeline_inner(
                renderer,
                &pll,
                vert_entry_point,
                forward_pass_vert,
                frag_entry_point,
                forward_pass_frag,
                blend,
                use_prepass,
                primitive_topology,
                label,
                samples,
            )
        };

        Self {
            pipeline_s1: inner(SampleCount::One),
            pipeline_s4: inner(SampleCount::Four),
            _phantom: PhantomData,
        }
    }

    /// Add the given routine to the graph with the given settings.
    ///
    /// I understand the requirement that extra_bgs is explicitly a Vec is
    /// weird, but due to my lifetime passthrough logic I can't pass through a
    /// slice.
    #[allow(clippy::too_many_arguments)]
    pub fn add_forward_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        forward_uniform_bg: DataHandle<BindGroup>,
        culled: DataHandle<culling::DrawCallSet>,
        per_material: &'node PerMaterialArchetypeInterface<M>,
        extra_bgs: Option<&'node Vec<BindGroup>>,
        label: &str,
        samples: SampleCount,
        color: RenderTargetHandle,
        resolve: Option<RenderTargetHandle>,
        depth: RenderTargetHandle,
    ) {
        let mut builder = graph.add_node(label);

        let hdr_color_handle = builder.add_render_target_output(color);
        let hdr_resolve = builder.add_optional_render_target_output(resolve);
        let hdr_depth_handle = builder.add_render_target_output(depth);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: hdr_color_handle,
                clear: Color::BLACK,
                resolve: hdr_resolve,
            }],
            depth_stencil: Some(RenderPassDepthTarget {
                target: DepthHandle::RenderTarget(hdr_depth_handle),
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        let forward_uniform_handle = builder.add_data_input(forward_uniform_bg);
        let cull_handle = builder.add_data_input(culled);

        let this_pt_handle = builder.passthrough_ref(self);
        let extra_bg_pt_handle = extra_bgs.map(|v| builder.passthrough_ref(v));

        builder.build(move |pt, renderer, encoder_or_pass, temps, ready, graph_data| {
            let this = pt.get(this_pt_handle);
            let extra_bgs = extra_bg_pt_handle.map(|h| pt.get(h));
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_handle).unwrap();
            let culled = graph_data.get_data(temps, cull_handle).unwrap();

            let per_material_bg = temps.add(
                BindGroupBuilder::new()
                    .append_buffer(graph_data.object_manager.buffer::<M>())
                    .append_buffer_with_size(
                        &culled.object_reference_buffer,
                        culling::ShaderBatchData::SHADER_SIZE.get(),
                    )
                    .append_buffer(graph_data.mesh_manager.buffer())
                    .append_buffer(graph_data.material_manager.archetype_view::<M>().buffer())
                    .build(&renderer.device, Some("Per-Material BG"), &per_material.bgl),
            );

            let pipeline = match samples {
                SampleCount::One => &this.pipeline_s1,
                SampleCount::Four => &this.pipeline_s4,
            };

            rpass.set_index_buffer(culled.index_buffer.slice(..), IndexFormat::Uint32);
            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, forward_uniform_bg, &[]);
            if let Some(v) = extra_bgs {
                for (idx, bg) in v.iter().enumerate() {
                    rpass.set_bind_group((idx + 3) as _, bg, &[])
                }
            }
            if let ProfileData::Gpu(ref bg) = ready.d2_texture.bg {
                rpass.set_bind_group(2, bg, &[]);
            }

            for (idx, call) in culled.draw_calls.iter().enumerate() {
                let call: &DrawCall = call;

                if renderer.profile.is_cpu_driven() {
                    rpass.set_bind_group(
                        2,
                        graph_data.material_manager.texture_bind_group(call.bind_group_index),
                        &[],
                    );
                }
                rpass.set_bind_group(
                    1,
                    per_material_bg,
                    &[idx as u32 * culling::ShaderBatchData::SHADER_SIZE.get() as u32],
                );
                rpass.draw_indexed(call.index_range.clone(), 0, 0..1);
            }
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn build_forward_pipeline_inner(
    renderer: &Renderer,
    pll: &wgpu::PipelineLayout,
    vert_entry_point: &str,
    forward_pass_vert: &wgpu::ShaderModule,
    frag_entry_point: &str,
    forward_pass_frag: &wgpu::ShaderModule,
    blend: Option<BlendState>,
    use_prepass: bool,
    primitive_topology: PrimitiveTopology,
    label: &str,
    samples: SampleCount,
) -> RenderPipeline {
    renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(pll),
        vertex: VertexState {
            module: forward_pass_vert,
            entry_point: vert_entry_point,
            buffers: &[],
        },
        primitive: PrimitiveState {
            topology: primitive_topology,
            strip_index_format: None,
            front_face: match renderer.handedness {
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
            depth_write_enabled: blend.is_none() && use_prepass,
            depth_compare: match use_prepass {
                true => CompareFunction::Equal,
                false => CompareFunction::GreaterEqual,
            },
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        }),
        multisample: MultisampleState {
            count: samples as u32,
            ..Default::default()
        },
        fragment: Some(FragmentState {
            module: forward_pass_frag,
            entry_point: frag_entry_point,
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend,
                write_mask: ColorWrites::all(),
            })],
        }),
        multiview: None,
    })
}
