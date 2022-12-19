//! Tonemapper which blits an image while applying a tonemapping operator.
//!
//! As of right now there is no tonemapping applied as we don't have
//! auto-exposure yet. Once we have auto-exposure, we can do proper tonemapping,
//! and will offer a variety of tonemapping operators.
//!
//! When creating the tonemapping, ensure you use the correct format for the
//! output. Each TonemappingRoutine instance only has a single pipeline, so if
//! you need to render to two different formats potentially, use two different
//! routines.

use std::borrow::Cow;

use rend3::{
    graph::{DataHandle, NodeResourceUsage, RenderGraph, RenderPassTarget, RenderPassTargets, RenderTargetHandle},
    util::bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
    Renderer, ShaderConfig, ShaderPreProcessor,
};
use wgpu::{
    BindGroup, BindGroupLayout, BindingType, Color, ColorTargetState, ColorWrites, Device, FragmentState, FrontFace,
    MultisampleState, PipelineLayoutDescriptor, PolygonMode, PrimitiveState, PrimitiveTopology, RenderPipeline,
    RenderPipelineDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages, TextureFormat, TextureSampleType,
    TextureViewDimension, VertexState,
};

use crate::common::WholeFrameInterfaces;

fn create_pipeline(
    device: &Device,
    spp: &ShaderPreProcessor,
    interfaces: &WholeFrameInterfaces,
    bgl: &BindGroupLayout,
    output_format: TextureFormat,
) -> RenderPipeline {
    profiling::scope!("TonemappingPass::new");
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("tonemapping"),
        source: ShaderSource::Wgsl(Cow::Owned(
            spp.render_shader("rend3-routine/blit.wgsl", &ShaderConfig::default(), None)
                .unwrap(),
        )),
    });

    let fs_entry_point = if output_format.describe().srgb {
        "fs_main_scene"
    } else {
        "fs_main_monitor"
    };

    let pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("tonemapping pass"),
        bind_group_layouts: &[&interfaces.forward_uniform_bgl, bgl],
        push_constant_ranges: &[],
    });

    device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("tonemapping pass"),
        layout: Some(&pll),
        vertex: VertexState {
            module: &module,
            entry_point: "vs_main",
            buffers: &[],
        },
        primitive: PrimitiveState {
            topology: PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: FrontFace::Cw,
            cull_mode: None,
            unclipped_depth: false,
            polygon_mode: PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(FragmentState {
            module: &module,
            entry_point: fs_entry_point,
            targets: &[Some(ColorTargetState {
                format: output_format,
                blend: None,
                write_mask: ColorWrites::all(),
            })],
        }),
        multiview: None,
    })
}

/// HDR tonemapping routine.
///
/// See module for documentation.
pub struct TonemappingRoutine {
    bgl: BindGroupLayout,
    pipeline: RenderPipeline,
}

impl TonemappingRoutine {
    pub fn new(
        renderer: &Renderer,
        spp: &ShaderPreProcessor,
        interfaces: &WholeFrameInterfaces,
        output_format: TextureFormat,
    ) -> Self {
        let bgl = BindGroupLayoutBuilder::new()
            .append(
                ShaderStages::FRAGMENT,
                BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                None,
            )
            .build(&renderer.device, Some("bind bgl"));

        let pipeline = create_pipeline(&renderer.device, spp, interfaces, &bgl, output_format);

        Self { bgl, pipeline }
    }

    pub fn add_to_graph<'node>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        src: RenderTargetHandle,
        dst: RenderTargetHandle,
        forward_uniform_bg: DataHandle<BindGroup>,
    ) {
        let mut builder = graph.add_node("Tonemapping");

        let input_handle = builder.add_render_target(src, NodeResourceUsage::Input);
        let output_handle = builder.add_render_target(dst, NodeResourceUsage::Output);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: None,
        });

        let forward_uniform_handle = builder.add_data(forward_uniform_bg, NodeResourceUsage::Input);

        let pt_handle = builder.passthrough_ref(self);

        builder.build(move |pt, renderer, encoder_or_pass, temps, _ready, graph_data| {
            let this = pt.get(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);
            let forward_uniform_bg = graph_data.get_data(temps, forward_uniform_handle).unwrap();
            let hdr_color = graph_data.get_render_target(input_handle);

            profiling::scope!("tonemapping");

            let blit_src_bg = temps.add(BindGroupBuilder::new().append_texture_view(hdr_color).build(
                &renderer.device,
                Some("blit src bg"),
                &this.bgl,
            ));

            rpass.set_pipeline(&this.pipeline);
            rpass.set_bind_group(0, forward_uniform_bg, &[]);
            rpass.set_bind_group(1, blit_src_bg, &[]);
            rpass.draw(0..3, 0..1);
        });
    }
}
