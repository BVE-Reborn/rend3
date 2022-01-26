//! Material agnostic routine for forward rendering.
//!
//! Will default to the PBR shader code if custom code is not specified.

use std::marker::PhantomData;

use arrayvec::ArrayVec;
use rend3::{
    graph::{
        DataHandle, DepthHandle, RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets,
        RenderTargetHandle,
    },
    types::{Handedness, Material, SampleCount},
    ProfileData, Renderer, RendererDataCore, RendererProfile,
};
use wgpu::{
    BindGroup, BindGroupLayout, BlendState, Color, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Face, FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderModule, StencilState,
    TextureFormat, VertexState,
};

use crate::{
    common::{
        profile_safe_shader, PerMaterialArchetypeInterface, WholeFrameInterfaces, CPU_VERTEX_BUFFERS,
        GPU_VERTEX_BUFFERS,
    },
    culling,
    pbr::PbrMaterial,
};

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
        interfaces: &WholeFrameInterfaces,
        per_material: &PerMaterialArchetypeInterface<M>,

        vertex: Option<(&str, &ShaderModule)>,
        fragment: Option<(&str, &ShaderModule)>,
        extra_bgls: &[BindGroupLayout],

        blend: Option<BlendState>,
        use_prepass: bool,
        label: &str,
    ) -> Self {
        profiling::scope!("PrimaryPasses::new");

        let _forward_pass_vert_owned;
        let forward_pass_vert;
        let vert_entry_point;
        match vertex {
            Some((inner_name, inner)) => {
                _forward_pass_vert_owned = None;
                forward_pass_vert = inner;
                vert_entry_point = inner_name;
            }
            None => {
                _forward_pass_vert_owned = Some(unsafe {
                    profile_safe_shader(
                        &renderer.device,
                        renderer.profile,
                        "forward pass vert",
                        "opaque.vert.cpu.wgsl",
                        "opaque.vert.gpu.spv",
                    )
                });
                vert_entry_point = "main";
                forward_pass_vert = _forward_pass_vert_owned.as_ref().unwrap();
            }
        };

        let _forward_pass_frag_owned;
        let forward_pass_frag;
        let frag_entry_point;
        match fragment {
            Some((inner_name, inner)) => {
                _forward_pass_frag_owned = None;
                forward_pass_frag = inner;
                frag_entry_point = inner_name;
            }
            None => {
                _forward_pass_frag_owned = Some(unsafe {
                    profile_safe_shader(
                        &renderer.device,
                        renderer.profile,
                        "forward pass frag",
                        "opaque.frag.cpu.wgsl",
                        "opaque.frag.gpu.spv",
                    )
                });
                frag_entry_point = "main";
                forward_pass_frag = _forward_pass_frag_owned.as_ref().unwrap()
            }
        };

        let mut bgls: ArrayVec<&BindGroupLayout, 8> = ArrayVec::new();
        bgls.push(&interfaces.forward_uniform_bgl);
        bgls.push(&per_material.bgl);
        if renderer.profile == RendererProfile::GpuDriven {
            bgls.push(data_core.d2_texture_manager.gpu_bgl())
        } else {
            bgls.push(data_core.material_manager.get_bind_group_layout_cpu::<PbrMaterial>());
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
        culled: DataHandle<culling::PerMaterialArchetypeData>,
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

        let _ = builder.add_shadow_array_input();

        let forward_uniform_handle = builder.add_data_input(forward_uniform_bg);
        let cull_handle = builder.add_data_input(culled);

        let this_pt_handle = builder.passthrough_ref(self);
        let extra_bg_pt_handle = extra_bgs.map(|v| builder.passthrough_ref(v));

        builder.build(move |pt, _renderer, encoder_or_pass, temps, ready, graph_data| {
            let this = pt.get(this_pt_handle);
            let extra_bgs = extra_bg_pt_handle.map(|h| pt.get(h));
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_handle).unwrap();
            let culled = graph_data.get_data(temps, cull_handle).unwrap();

            let pipeline = match samples {
                SampleCount::One => &this.pipeline_s1,
                SampleCount::Four => &this.pipeline_s4,
            };

            graph_data.mesh_manager.buffers().bind(rpass);

            rpass.set_pipeline(pipeline);
            rpass.set_bind_group(0, forward_uniform_bg, &[]);
            rpass.set_bind_group(1, &culled.per_material, &[]);
            if let Some(v) = extra_bgs {
                for (idx, bg) in v.iter().enumerate() {
                    rpass.set_bind_group((idx + 3) as _, bg, &[])
                }
            }

            match culled.inner.calls {
                ProfileData::Cpu(ref draws) => {
                    culling::draw_cpu_powered::<PbrMaterial>(rpass, draws, graph_data.material_manager, 2)
                }
                ProfileData::Gpu(ref data) => {
                    rpass.set_bind_group(2, ready.d2_texture.bg.as_gpu(), &[]);
                    culling::draw_gpu_powered(rpass, data);
                }
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
    label: &str,
    samples: SampleCount,
) -> RenderPipeline {
    renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(pll),
        vertex: VertexState {
            module: forward_pass_vert,
            entry_point: vert_entry_point,
            buffers: match renderer.profile {
                RendererProfile::CpuDriven => &CPU_VERTEX_BUFFERS,
                RendererProfile::GpuDriven => &GPU_VERTEX_BUFFERS,
            },
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
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
            targets: &[ColorTargetState {
                format: TextureFormat::Rgba16Float,
                blend,
                write_mask: ColorWrites::all(),
            }],
        }),
        multiview: None,
    })
}
