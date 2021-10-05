use egui_wgpu_backend::ScreenDescriptor;
use rend3::{RenderRoutine, Renderer};
use wgpu::{TextureFormat, TextureView};

pub struct EguiRenderRoutine {
    internal: egui_wgpu_backend::RenderPass,
    screen_descriptor: egui_wgpu_backend::ScreenDescriptor,
}

impl EguiRenderRoutine {
    pub fn new(
        renderer: &Renderer,
        surface_format: TextureFormat,
        msaa_samples: u32,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> Self {
        let rpass = egui_wgpu_backend::RenderPass::new(&renderer.device, surface_format, msaa_samples);

        Self {
            internal: rpass,
            screen_descriptor: egui_wgpu_backend::ScreenDescriptor {
                physical_height: height,
                physical_width: width,
                scale_factor,
            },
        }
    }
}

pub struct Input<'a> {
    pub clipped_meshes: &'a Vec<egui::ClippedMesh>,
    pub context: egui::CtxRef,
}

impl EguiRenderRoutine {
    pub fn resize(&mut self, new_width: u32, new_height: u32, new_scale_factor: f32) {
        self.screen_descriptor = ScreenDescriptor {
            physical_height: new_height,
            physical_width: new_width,
            scale_factor: new_scale_factor,
        };
    }
}

impl RenderRoutine<&Input<'_>, &TextureView> for EguiRenderRoutine {
    fn render(
        &mut self,
        renderer: std::sync::Arc<rend3::Renderer>,
        cmd_bufs: flume::Sender<wgpu::CommandBuffer>,
        _ready: rend3::ManagerReadyOutput,
        input: &Input<'_>,
        output: &TextureView,
    ) {
        let mut encoder = renderer.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("egui command encoder"),
        });

        self.internal
            .update_texture(&renderer.device, &renderer.queue, &input.context.texture());
        self.internal.update_user_textures(&renderer.device, &renderer.queue);
        self.internal.update_buffers(
            &renderer.device,
            &renderer.queue,
            &input.clipped_meshes,
            &self.screen_descriptor,
        );

        self.internal
            .execute(
                &mut encoder,
                &output,
                &input.clipped_meshes,
                &self.screen_descriptor,
                None,
            )
            .unwrap(); // TODO don't unwrap

        cmd_bufs.send(encoder.finish()).unwrap();
    }
}

impl epi::TextureAllocator for EguiRenderRoutine {
    fn alloc_srgba_premultiplied(&mut self, size: (usize, usize), srgba_pixels: &[egui::Color32]) -> egui::TextureId {
        self.internal.alloc_srgba_premultiplied(size, srgba_pixels)
    }

    fn free(&mut self, id: egui::TextureId) {
        self.internal.free(id);
    }
}
