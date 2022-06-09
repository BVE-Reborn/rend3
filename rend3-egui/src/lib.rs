//! Render routine integrating egui into a rend3 rendergraph.
//!
//! Call [`EguiRenderRoutine::add_to_graph`] to add it to the graph.

use egui::TexturesDelta;
use rend3::{
    graph::{RenderGraph, RenderPassTarget, RenderPassTargets, RenderTargetHandle},
    types::SampleCount,
    Renderer,
};
use std::{mem, sync::Arc};
use wgpu::{Color, TextureFormat};

pub struct EguiRenderRoutine {
    pub internal: egui_wgpu::renderer::RenderPass,
    screen_descriptor: egui_wgpu::renderer::ScreenDescriptor,
    textures_to_free: Vec<egui::TextureId>,
}

impl EguiRenderRoutine {
    /// Creates a new render routine to render a egui UI.
    ///
    /// Egui will always output gamma-encoded color. It will determine if to do
    /// this in the shader manually based on the output format.
    pub fn new(
        renderer: &Renderer,
        surface_format: TextureFormat,
        samples: SampleCount,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> Self {
        let rpass = egui_wgpu::renderer::RenderPass::new(&renderer.device, surface_format, samples as _);

        Self {
            internal: rpass,
            screen_descriptor: egui_wgpu::renderer::ScreenDescriptor {
                size_in_pixels: [width, height],
                pixels_per_point: scale_factor,
            },
            textures_to_free: Vec::new(),
        }
    }

    pub fn resize(&mut self, new_width: u32, new_height: u32, new_scale_factor: f32) {
        self.screen_descriptor = egui_wgpu::renderer::ScreenDescriptor {
            size_in_pixels: [new_width, new_height],
            pixels_per_point: new_scale_factor,
        };
    }

    pub fn add_to_graph<'node>(
        &'node mut self,
        graph: &mut RenderGraph<'node>,
        mut input: Input<'node>,
        output: RenderTargetHandle,
    ) {
        let mut builder = graph.add_node("egui");

        let output_handle = builder.add_render_target_output(output);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: None,
        });

        // We can't free textures directly after the call to `execute_with_renderpass` as it freezes
        // the lifetime of `self` for the remainder of the closure. so we instead buffer the textures
        // to free for a frame so we can clean them up before the next call.
        let textures_to_free = mem::replace(&mut self.textures_to_free, mem::take(&mut input.textures_delta.free));
        let pt_handle = builder.passthrough_ref_mut(self);

        builder.build(move |pt, renderer, encoder_or_pass, _temps, _ready, _graph_data| {
            let this = pt.get_mut(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);

            for tex in textures_to_free {
                this.internal.free_texture(&tex);
            }
            for (id, image_delta) in input.textures_delta.set {
                this.internal
                    .update_texture(&renderer.device, &renderer.queue, id, &image_delta)
            }

            this.internal.update_buffers(
                &renderer.device,
                &renderer.queue,
                input.paint_jobs,
                &this.screen_descriptor,
            );

            this.internal
                .execute_with_renderpass(rpass, input.paint_jobs, &this.screen_descriptor);
        });
    }

    /// Creates an egui texture from the given image data, format, and dimensions.
    pub fn create_egui_texture(
        internal: &mut egui_wgpu::renderer::RenderPass,
        renderer: &Arc<rend3::Renderer>,
        format: wgpu::TextureFormat,
        image_rgba: &[u8],
        dimensions: (u32, u32),
        label: Option<&str>,
    ) -> egui::TextureId {
        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        let image_texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label,
        });

        Self::wgpu_texture_to_egui(
            internal,
            renderer,
            image_texture,
            image_rgba,
            dimensions,
            format.describe(),
        )
    }

    /// Creates egui::TextureId with wgpu backend with existing wgpu::Texture
    pub fn wgpu_texture_to_egui(
        internal: &mut egui_wgpu::renderer::RenderPass,
        renderer: &Arc<rend3::Renderer>,
        image_texture: wgpu::Texture,
        image_rgba: &[u8],
        dimensions: (u32, u32),
        textureformatinfo: wgpu_types::TextureFormatInfo,
    ) -> egui::TextureId {
        let device = &renderer.device;
        let queue = &renderer.queue;

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &image_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            image_rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(
                    (dimensions.0 / textureformatinfo.block_dimensions.0 as u32) * textureformatinfo.block_size as u32,
                ),
                rows_per_image: None,
            },
            texture_size,
        );

        egui_wgpu::renderer::RenderPass::register_native_texture(
            internal,
            device,
            &image_texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                ..Default::default()
            }),
            wgpu::FilterMode::Linear,
        )
    }
}

pub struct Input<'a> {
    pub paint_jobs: &'a Vec<egui::epaint::ClippedPrimitive>,
    pub textures_delta: TexturesDelta,
    pub context: egui::Context,
}
