use crate::{
    list::{ImageOutputReference, RenderPass},
    renderer::list::RenderListCache,
    Renderer,
};
use parking_lot::RwLock;
use std::sync::Arc;
use wgpu::{
    CommandEncoderDescriptor, Operations, RenderPassColorAttachmentDescriptor,
    RenderPassDepthStencilAttachmentDescriptor, RenderPassDescriptor, SwapChainFrame,
};

pub(crate) async fn render_single_renderpass<TD>(
    renderer: Arc<Renderer<TD>>,
    pass: RenderPass,
    cache: Arc<RwLock<RenderListCache>>,
    frame: Arc<SwapChainFrame>,
) where
    TD: 'static,
{
    let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("single renderpass encoder"),
    });

    let cache_guard = cache.read();

    let colors: Vec<_> = pass
        .desc
        .outputs
        .iter()
        .map(|out| RenderPassColorAttachmentDescriptor {
            attachment: match out.output {
                ImageOutputReference::OutputImage => &frame.output.view,
                ImageOutputReference::Custom(ref name) => cache_guard.get_image(name),
            },
            resolve_target: out.resolve_target.as_ref().map(|depth| match depth {
                ImageOutputReference::OutputImage => &frame.output.view,
                ImageOutputReference::Custom(ref name) => cache_guard.get_image(name),
            }),
            ops: Operations {
                load: out.clear,
                store: true,
            },
        })
        .collect();

    let depth = pass
        .desc
        .depth
        .as_ref()
        .map(|depth| RenderPassDepthStencilAttachmentDescriptor {
            attachment: match depth.output {
                ImageOutputReference::OutputImage => &frame.output.view,
                ImageOutputReference::Custom(ref name) => cache_guard.get_image(name),
            },
            depth_ops: Some(Operations {
                load: depth.clear,
                store: true,
            }),
            stencil_ops: None,
        });

    let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
        color_attachments: &colors,
        depth_stencil_attachment: depth,
    });

    drop(rpass);
}
