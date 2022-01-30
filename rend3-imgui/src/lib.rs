//! Render routine integrating imgui into a rend3 rendergraph.
//!
//! Call [`ImguiRenderRoutine::add_to_graph`] to add it to the graph.

use imgui_wgpu::RendererConfig;
use rend3::{
    graph::{RenderGraph, RenderPassTarget, RenderPassTargets, RenderTargetHandle},
    types::{Color, TextureFormat},
    Renderer,
};

/// An instance of a render routine, holding in imgui_wgpu renderer.
pub struct ImguiRenderRoutine {
    pub renderer: imgui_wgpu::Renderer,
}

impl ImguiRenderRoutine {
    /// Imgui will always output gamma-encoded color. It will determine if to do
    /// this in the shader manually based on the output format.
    pub fn new(renderer: &Renderer, imgui: &mut imgui::Context, output_format: TextureFormat) -> Self {
        let base = if output_format.describe().srgb {
            RendererConfig::new()
        } else {
            RendererConfig::new_srgb()
        };

        let renderer = imgui_wgpu::Renderer::new(
            imgui,
            &renderer.device,
            &renderer.queue,
            RendererConfig {
                texture_format: output_format,
                ..base
            },
        );

        Self { renderer }
    }

    pub fn add_to_graph<'node>(
        &'node mut self,
        graph: &mut RenderGraph<'node>,
        draw_data: &'node imgui::DrawData,
        output: RenderTargetHandle,
    ) {
        let mut builder = graph.add_node("imgui");

        let output_handle = builder.add_render_target_output(output);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: None,
        });

        let pt_handle = builder.passthrough_ref_mut(self);

        builder.build(move |pt, renderer, encoder_or_pass, _temps, _ready, _graph_data| {
            let this = pt.get_mut(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);

            this.renderer
                .render(draw_data, &renderer.queue, &renderer.device, rpass)
                .unwrap();
        })
    }
}
