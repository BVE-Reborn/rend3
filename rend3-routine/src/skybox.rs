//! Routine that renders a cubemap as a skybox.

use std::borrow::Cow;

use rend3::{
    graph::{
        DataHandle, DepthHandle, RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets,
        RenderTargetHandle,
    },
    types::{SampleCount, TextureHandle},
    util::bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
    Renderer,
};
use wgpu::{
    BindGroup, BindGroupLayout, BindingType, Color, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Face, FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, StencilState, TextureFormat, TextureSampleType, TextureViewDimension, VertexState,
};

use crate::{common::WholeFrameInterfaces, shaders::WGSL_SHADERS};

struct StoredSkybox {
    bg: Option<BindGroup>,
    handle: Option<TextureHandle>,
}

/// Skybox rendering routine.
///
/// See module for documentation.
pub struct SkyboxRoutine {
    pipelines: SkyboxPipelines,
    bgl: BindGroupLayout,
    current_skybox: StoredSkybox,
}

impl SkyboxRoutine {
    /// Create the routine.
    pub fn new(renderer: &Renderer, interfaces: &WholeFrameInterfaces) -> Self {
        let bgl = BindGroupLayoutBuilder::new()
            .append(
                ShaderStages::FRAGMENT,
                BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::Cube,
                    multisampled: false,
                },
                None,
            )
            .build(&renderer.device, Some("skybox bgl"));

        let pipelines = SkyboxPipelines::new(renderer, interfaces, &bgl);

        Self {
            current_skybox: StoredSkybox { bg: None, handle: None },
            bgl,
            pipelines,
        }
    }

    /// Set the current background texture. Bad things will happen if this isn't
    /// a cube texture.
    pub fn set_background_texture(&mut self, texture: Option<TextureHandle>) {
        self.current_skybox.handle = texture;
        self.current_skybox.bg = None;
    }

    /// Update data if the background has changed since last frame.
    pub fn ready(&mut self, renderer: &Renderer) {
        let data_core = renderer.data_core.lock();
        let d2c_texture_manager = &data_core.d2c_texture_manager;

        profiling::scope!("Update Skybox");

        if let Some(ref handle) = self.current_skybox.handle {
            if self.current_skybox.bg.is_none() {
                let bg = BindGroupBuilder::new()
                    .append_texture_view(d2c_texture_manager.get_view(handle.get_raw()))
                    .build(&renderer.device, Some("skybox"), &self.bgl);

                self.current_skybox.bg = Some(bg)
            }
        }
    }

    /// Add rendering the skybox to the given rendergraph.
    pub fn add_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        color: RenderTargetHandle,
        resolve: Option<RenderTargetHandle>,
        depth: RenderTargetHandle,
        forward_uniform_bg: DataHandle<BindGroup>,
        samples: SampleCount,
    ) {
        let mut builder = graph.add_node("Skybox");

        let hdr_color_handle = builder.add_render_target_output(color);
        let hdr_resolve = builder.add_optional_render_target_output(resolve);
        let hdr_depth_handle = builder.add_render_target_input(depth);

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
        let pt_handle = builder.passthrough_ref(self);

        builder.build(move |pt, _renderer, encoder_or_pass, temps, _ready, graph_data| {
            let this = pt.get(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);

            let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_handle).unwrap();

            if let Some(ref bg) = this.current_skybox.bg {
                let pipeline = match samples {
                    SampleCount::One => &this.pipelines.pipeline_s1,
                    SampleCount::Four => &this.pipelines.pipeline_s4,
                };

                rpass.set_pipeline(pipeline);
                rpass.set_bind_group(0, forward_uniform_bg, &[]);
                rpass.set_bind_group(1, bg, &[]);
                rpass.draw(0..3, 0..1);
            }
        });
    }
}

/// Container for all needed skybox pipelines
pub struct SkyboxPipelines {
    pub pipeline_s1: RenderPipeline,
    pub pipeline_s4: RenderPipeline,
}
impl SkyboxPipelines {
    pub fn new(renderer: &Renderer, interfaces: &WholeFrameInterfaces, bgl: &BindGroupLayout) -> Self {
        profiling::scope!("build skybox pipeline");
        let skybox_pass_vert = renderer.device.create_shader_module(&ShaderModuleDescriptor {
            label: Some("skybox vert"),
            source: ShaderSource::Wgsl(Cow::Borrowed(
                WGSL_SHADERS
                    .get_file("skybox.vert.wgsl")
                    .unwrap()
                    .contents_utf8()
                    .unwrap(),
            )),
        });
        let skybox_pass_frag = renderer.device.create_shader_module(&ShaderModuleDescriptor {
            label: Some("skybox frag"),
            source: ShaderSource::Wgsl(Cow::Borrowed(
                WGSL_SHADERS
                    .get_file("skybox.frag.wgsl")
                    .unwrap()
                    .contents_utf8()
                    .unwrap(),
            )),
        });

        let pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("skybox pass"),
            bind_group_layouts: &[&interfaces.forward_uniform_bgl, bgl],
            push_constant_ranges: &[],
        });

        let inner = |samples| {
            renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("skybox pass"),
                layout: Some(&pll),
                vertex: VertexState {
                    module: &skybox_pass_vert,
                    entry_point: "main",
                    buffers: &[],
                },
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: FrontFace::Cw,
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
                    bias: DepthBiasState::default(),
                }),
                multisample: MultisampleState {
                    count: samples as u32,
                    ..Default::default()
                },
                fragment: Some(FragmentState {
                    module: &skybox_pass_frag,
                    entry_point: "main",
                    targets: &[ColorTargetState {
                        format: TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: ColorWrites::all(),
                    }],
                }),
                multiview: None,
            })
        };

        Self {
            pipeline_s1: inner(SampleCount::One),
            pipeline_s4: inner(SampleCount::Four),
        }
    }
}
