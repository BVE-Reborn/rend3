use crate::{
    instruction::Instruction,
    renderer::{BUFFER_RECALL_PRIORITY, COMPUTE_POOL},
    statistics::RendererStatistics,
    Renderer, TLS,
};
use std::{future::Future, sync::Arc};
use tracing_futures::Instrument;
use wgpu::{
    Color, CommandEncoderDescriptor, Extent3d, LoadOp, Operations, Origin3d, RenderPassColorAttachmentDescriptor,
    RenderPassDescriptor, TextureCopyView, TextureDataLayout, TextureDescriptor, TextureDimension, TextureUsage,
    TextureViewDescriptor,
};

pub fn render_loop<TLD>(renderer: Arc<Renderer<TLD>>) -> impl Future<Output = RendererStatistics>
where
    TLD: AsMut<TLS> + 'static,
{
    let span = tracing::debug_span!("Render Loop Creation");
    let _guard = span.enter();

    // blocks, do it before we async
    renderer.instructions.swap();

    let render_loop_span = tracing::warn_span!("Render Loop");
    async move {
        let mut instructions = renderer.instructions.consumer.lock();

        let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        span!(event_guard, INFO, "Process events");

        let mut new_options = None;

        let mut mesh_manager = renderer.mesh_manager.write();
        let mut texture_manager = renderer.texture_manager.write();
        let mut material_manager = renderer.material_manager.write();
        let mut object_manager = renderer.object_manager.write();

        for cmd in instructions.drain(..) {
            match cmd {
                Instruction::AddMesh { handle, mesh } => {
                    mesh_manager.fill(&renderer.queue, handle, mesh);
                }
                Instruction::RemoveMesh { handle } => {
                    mesh_manager.remove(handle);
                }
                Instruction::AddTexture { handle, texture } => {
                    let size = Extent3d {
                        width: texture.width,
                        height: texture.height,
                        depth: 1,
                    };

                    let uploaded_tex = renderer.device.create_texture(&TextureDescriptor {
                        label: None,
                        size,
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: TextureDimension::D2,
                        format: texture.format.into(),
                        usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
                    });

                    renderer.queue.write_texture(
                        TextureCopyView {
                            texture: &uploaded_tex,
                            origin: Origin3d::ZERO,
                            mip_level: 0,
                        },
                        &texture.data,
                        TextureDataLayout {
                            offset: 0,
                            bytes_per_row: texture.format.bytes_per_pixel() * texture.width,
                            rows_per_image: 0,
                        },
                        size,
                    );

                    texture_manager.fill(handle, uploaded_tex.create_view(&TextureViewDescriptor::default()));
                }
                Instruction::RemoveTexture { handle } => {
                    texture_manager.remove(handle);
                }
                Instruction::AddMaterial { handle, material } => {
                    material_manager.fill(handle, material);
                }
                Instruction::RemoveMaterial { handle } => {
                    material_manager.remove(handle);
                }
                Instruction::AddObject { handle, object } => {
                    object_manager.fill(handle, object, &mesh_manager);
                }
                Instruction::SetObjectTransform {
                    handle: object,
                    transform,
                } => {
                    object_manager.set_object_transform(object, transform);
                }
                Instruction::RemoveObject { handle } => {
                    object_manager.remove(handle);
                }
                Instruction::SetOptions { options } => new_options = Some(options),
            }
        }

        texture_manager.ready(&renderer.device);
        material_manager.ready(&renderer.device, &mut encoder, &texture_manager);
        object_manager.ready(&renderer.device, &mut encoder, &material_manager);

        drop((mesh_manager, texture_manager, material_manager, object_manager));

        drop(event_guard);
        span!(global_resource_guard, INFO, "Update global resources");

        if let Some(ref new_opt) = new_options {
            renderer
                .global_resources
                .write()
                .update(&renderer.device, &renderer.surface, &renderer.options, new_opt);
        }

        drop(global_resource_guard);
        span!(renderpass_guard, INFO, "Primary Renderpass");

        let frame = renderer.global_resources.write().swapchain.get_current_frame().unwrap();

        let rpass = encoder.begin_render_pass(&RenderPassDescriptor {
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
        drop(renderpass_guard);

        renderer.queue.submit(std::iter::once(encoder.finish()));

        span!(buffer_pump_guard, INFO, "Pumping Buffers");

        let futures = renderer.buffer_manager.lock().pump();
        for future in futures {
            let span = tracing::debug_span!("Buffer recall");
            renderer
                .yard
                .spawn(COMPUTE_POOL, BUFFER_RECALL_PRIORITY, future.instrument(span));
        }
        drop(buffer_pump_guard);

        drop(frame); // present

        RendererStatistics {}
    }
    .instrument(render_loop_span)
}
