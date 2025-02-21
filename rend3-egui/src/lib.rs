//! Render routine integrating egui into a rend3 rendergraph.
//!
//! Call [`EguiRenderRoutine::add_to_graph`] to add it to the graph.

use std::{mem, sync::Arc};

use egui::TexturesDelta;
use glam::Vec4;
use rend3::{
    graph::{NodeResourceUsage, RenderGraph, RenderPassTarget, RenderPassTargets, RenderTargetHandle},
    types::SampleCount,
    Renderer,
};
use wgpu::TextureFormat;

pub struct EguiRenderRoutine {
    pub internal: egui_wgpu::Renderer,
    screen_descriptor: egui_wgpu::ScreenDescriptor,
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
        const DITHERING: bool = false;  // new EGUI feature.
        let rpass = egui_wgpu::Renderer::new(&renderer.device, surface_format, None, samples as _, DITHERING);

        Self {
            internal: rpass,
            screen_descriptor: egui_wgpu::ScreenDescriptor {
                size_in_pixels: [width, height],
                pixels_per_point: scale_factor,
            },
            textures_to_free: Vec::new(),
        }
    }

    pub fn resize(&mut self, new_width: u32, new_height: u32, new_scale_factor: f32) {
        self.screen_descriptor =
            egui_wgpu::ScreenDescriptor { size_in_pixels: [new_width, new_height], pixels_per_point: new_scale_factor };
    }

    pub fn add_to_graph<'node>(
        &'node mut self,
        graph: &mut RenderGraph<'node>,
        mut input: Input<'node>,
        output: RenderTargetHandle,
    ) {
        let mut builder = graph.add_node("egui");

        let rpass_handle = builder.add_renderpass(
            RenderPassTargets {
                targets: vec![RenderPassTarget { color: output, clear: Vec4::ZERO, resolve: None }],
                depth_stencil: None,
            },
            NodeResourceUsage::InputOutput,
        );

        // We can't free textures directly after the call to `execute_with_renderpass` as it freezes
        // the lifetime of `self` for the remainder of the closure. so we instead buffer the textures
        // to free for a frame so we can clean them up before the next call.
        let textures_to_free = mem::replace(&mut self.textures_to_free, mem::take(&mut input.textures_delta.free));

        builder.build(move |mut ctx| {
            let rpass = ctx.encoder_or_pass.take_rpass(rpass_handle);

            for texture in textures_to_free {
                self.internal.free_texture(&texture);
            }
            for (id, image_delta) in input.textures_delta.set {
                self.internal.update_texture(&ctx.renderer.device, &ctx.renderer.queue, id, &image_delta);
            }
            let mut cmd_buffer = ctx.renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            self.internal.update_buffers(
                &ctx.renderer.device,
                &ctx.renderer.queue,
                &mut cmd_buffer,
                input.clipped_meshes,
                &self.screen_descriptor,
            );
            drop(cmd_buffer);
            self.internal.render(&mut rpass.borrow_mut(), input.clipped_meshes, &self.screen_descriptor);
        });
    }

    /// Creates an egui texture from the given image data, format, and dimensions.
    pub fn create_egui_texture(
        internal: &mut egui_wgpu::Renderer,
        renderer: &Arc<rend3::Renderer>,
        format: wgpu::TextureFormat,
        image_rgba: &[u8],
        dimensions: (u32, u32),
        label: Option<&str>,
    ) -> egui::TextureId {
        let texture_size = wgpu::Extent3d { width: dimensions.0, height: dimensions.1, depth_or_array_layers: 1 };

        let image_texture = renderer.device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label,
            view_formats: &[],
        });

        EguiRenderRoutine::wgpu_texture_to_egui(
            internal,
            renderer,
            image_texture,
            image_rgba,
            dimensions,
            format.block_dimensions(),
            format.block_copy_size(None).unwrap(),
        )
    }

    /// Creates egui::TextureId with wgpu backend with existing wgpu::Texture
    pub fn wgpu_texture_to_egui(
        internal: &mut egui_wgpu::Renderer,
        renderer: &Arc<rend3::Renderer>,
        image_texture: wgpu::Texture,
        image_rgba: &[u8],
        dimensions: (u32, u32),
        block_dimensions: (u32, u32),
        block_size: u32,
    ) -> egui::TextureId {
        let device = &renderer.device;
        let queue = &renderer.queue;

        let texture_size = wgpu::Extent3d { width: dimensions.0, height: dimensions.1, depth_or_array_layers: 1 };

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &image_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            image_rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((dimensions.0 / block_dimensions.0) * block_size),
                rows_per_image: None,
            },
            texture_size,
        );

        internal.register_native_texture(
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
    pub clipped_meshes: &'a Vec<egui::ClippedPrimitive>,
    pub textures_delta: TexturesDelta,
    pub context: egui::Context,
}
