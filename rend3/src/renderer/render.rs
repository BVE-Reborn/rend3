use crate::{instruction::Instruction, statistics::RendererStatistics, Renderer, TLS};
use std::{future::Future, sync::Arc};
use wgpu::{
    Color, CommandEncoderDescriptor, LoadOp, Operations, RenderPassColorAttachmentDescriptor, RenderPassDescriptor,
};

pub fn render_loop<TLD>(renderer: Arc<Renderer<TLD>>) -> impl Future<Output = RendererStatistics>
where
    TLD: AsMut<TLS> + 'static,
{
    // blocks, do it before we async
    renderer.instructions.swap();

    async move {
        let mut instructions = renderer.instructions.consumer.lock();

        let mut new_options = None;

        for cmd in instructions.drain(..) {
            match cmd {
                Instruction::SetOptions { options } => new_options = Some(options),
                _ => unimplemented!(),
            }
        }

        if let Some(ref new_opt) = new_options {
            renderer
                .global_resources
                .write()
                .update(&renderer.device, &renderer.surface, &renderer.options, new_opt);
        }

        let frame = renderer.global_resources.write().swapchain.get_current_frame().unwrap();

        let mut command_encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        let rpass = command_encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[RenderPassColorAttachmentDescriptor {
                attachment: &frame.output.view,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: true,
                },
                resolve_target: None,
            }],
            depth_stencil_attachment: None,
        });

        drop(rpass);

        renderer.queue.submit(std::iter::once(command_encoder.finish()));

        drop(frame); // present

        RendererStatistics {}
    }
}
