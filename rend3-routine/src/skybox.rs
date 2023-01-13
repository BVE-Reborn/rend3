//! Routine that renders a cubemap as a skybox.

use std::borrow::Cow;

use rend3::{
    graph::{
        DataHandle, NodeResourceUsage, RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets,
        RenderTargetHandle,
    },
    types::{SampleCount, TextureCubeHandle},
    util::bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
    Renderer, ShaderConfig, ShaderPreProcessor,
};
use wgpu::{
    BindGroup, BindGroupLayout, BindingType, Color, ColorTargetState, ColorWrites, CompareFunction, DepthBiasState,
    DepthStencilState, Face, FragmentState, FrontFace, MultisampleState, PipelineLayoutDescriptor, PolygonMode,
    PrimitiveState, PrimitiveTopology, RenderPipeline, RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, StencilState, TextureFormat, TextureSampleType, TextureViewDimension, VertexState,
};

use crate::common::WholeFrameInterfaces;

struct StoredSkybox {
    bg: Option<BindGroup>,
    handle: Option<TextureCubeHandle>,
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
    pub fn new(renderer: &Renderer, spp: &ShaderPreProcessor, interfaces: &WholeFrameInterfaces) -> Self {
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

        let pipelines = SkyboxPipelines::new(renderer, spp, interfaces, &bgl);

        Self {
            current_skybox: StoredSkybox { bg: None, handle: None },
            bgl,
            pipelines,
        }
    }

    /// Set the current background texture. Bad things will happen if this isn't
    /// a cube texture.
    pub fn set_background_texture(&mut self, texture: Option<TextureCubeHandle>) {
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

        let hdr_color_handle = builder.add_render_target(color, NodeResourceUsage::InputOutput);
        let hdr_resolve = builder.add_optional_render_target(resolve, NodeResourceUsage::InputOutput);
        let hdr_depth_handle = builder.add_render_target(depth, NodeResourceUsage::Input);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: hdr_color_handle,
                clear: Color::BLACK,
                resolve: hdr_resolve,
            }],
            depth_stencil: Some(RenderPassDepthTarget {
                target: hdr_depth_handle,
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        let forward_uniform_handle = builder.add_data(forward_uniform_bg, NodeResourceUsage::Input);

        builder.build(move |mut ctx| {
            let rpass = ctx.encoder_or_pass.take_rpass(rpass_handle);

            let forward_uniform_bg = ctx.graph_data.get_data(ctx.temps, forward_uniform_handle).unwrap();

            if let Some(ref bg) = self.current_skybox.bg {
                let pipeline = match samples {
                    SampleCount::One => &self.pipelines.pipeline_s1,
                    SampleCount::Four => &self.pipelines.pipeline_s4,
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
    pub fn new(
        renderer: &Renderer,
        spp: &ShaderPreProcessor,
        interfaces: &WholeFrameInterfaces,
        bgl: &BindGroupLayout,
    ) -> Self {
        profiling::scope!("build skybox pipeline");
        let skybox_sm = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("skybox vert"),
            source: ShaderSource::Wgsl(Cow::Owned(
                spp.render_shader("rend3-routine/skybox.wgsl", &ShaderConfig::default(), None)
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
                    module: &skybox_sm,
                    entry_point: "vs_main",
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
                    module: &skybox_sm,
                    entry_point: "fs_main",
                    targets: &[Some(ColorTargetState {
                        format: TextureFormat::Rgba16Float,
                        blend: None,
                        write_mask: ColorWrites::all(),
                    })],
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
