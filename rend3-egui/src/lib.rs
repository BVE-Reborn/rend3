//! Render routine integrating egui into a rend3 rendergraph.
//!
//! Call [`EguiRenderRoutine::add_to_graph`] to add it to the graph.

use std::sync::Arc;
use rend3::{
    graph::{RenderGraph, RenderPassTarget, RenderPassTargets, RenderTargetHandle},
    types::SampleCount,
    Renderer,
};
use wgpu::{Color, TextureFormat};

pub struct EguiRenderRoutine {
    pub internal: egui_wgpu_backend::RenderPass,
    screen_descriptor: egui_wgpu_backend::ScreenDescriptor,
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
        let rpass = egui_wgpu_backend::RenderPass::new(&renderer.device, surface_format, samples as _);

        Self {
            internal: rpass,
            screen_descriptor: egui_wgpu_backend::ScreenDescriptor {
                physical_height: height,
                physical_width: width,
                scale_factor,
            },
        }
    }

    pub fn resize(&mut self, new_width: u32, new_height: u32, new_scale_factor: f32) {
        self.screen_descriptor = egui_wgpu_backend::ScreenDescriptor {
            physical_height: new_height,
            physical_width: new_width,
            scale_factor: new_scale_factor,
        };
    }

    pub fn add_to_graph<'node>(
        &'node mut self,
        graph: &mut RenderGraph<'node>,
        input: Input<'node>,
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

        let pt_handle = builder.passthrough_ref_mut(self);

        builder.build(move |pt, renderer, encoder_or_pass, _temps, _ready, _graph_data| {
            let this = pt.get_mut(pt_handle);
            let rpass = encoder_or_pass.get_rpass(rpass_handle);

            this.internal
                .update_texture(&renderer.device, &renderer.queue, &input.context.font_image());
            this.internal.update_user_textures(&renderer.device, &renderer.queue);
            this.internal.update_buffers(
                &renderer.device,
                &renderer.queue,
                input.clipped_meshes,
                &this.screen_descriptor,
            );

            this.internal
                .execute_with_renderpass(rpass, input.clipped_meshes, &this.screen_descriptor)
                .unwrap();
        });
    }

    pub fn image_to_egui(internal: &mut egui_wgpu_backend::RenderPass, renderer: &Arc<rend3::Renderer>, image_rgba: &image::ImageBuffer<image::Rgba<u8>, std::vec::Vec<u8>>, dimensions: (u32, u32),) -> egui::TextureId {
        let device = &renderer.device;
        let queue = &renderer.queue;

        let texture_size = wgpu::Extent3d {
            width: dimensions.0,
            height: dimensions.1,
            depth_or_array_layers: 1,
        };
        let image_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            label: Some("image_texture"),
        });

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
                bytes_per_row: std::num::NonZeroU32::new(4 * dimensions.0),
                rows_per_image: std::num::NonZeroU32::new(dimensions.1),
            },
            texture_size,
        );

        let egui_image = egui_wgpu_backend::RenderPass::egui_texture_from_wgpu_texture(internal, device, &image_texture, wgpu::FilterMode::Linear);

        return egui_image;
    }

    pub fn wgpu_texture_to_egui(internal: &mut egui_wgpu_backend::RenderPass, renderer: &Arc<rend3::Renderer>, image_texture: wgpu::Texture, image_rgba: &image::ImageBuffer<image::Rgba<u8>, std::vec::Vec<u8>>, dimensions: (u32, u32),) -> egui::TextureId {
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
                bytes_per_row: std::num::NonZeroU32::new(4 * dimensions.0),
                rows_per_image: std::num::NonZeroU32::new(dimensions.1),
            },
            texture_size,
        );
        
        let egui_image = egui_wgpu_backend::RenderPass::egui_texture_from_wgpu_texture(internal, device, &image_texture, wgpu::FilterMode::Linear);

        return egui_image;
    }
}

pub struct Input<'a> {
    pub clipped_meshes: &'a Vec<egui::ClippedMesh>,
    pub context: egui::CtxRef,
}
