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

        span_transfer!(_ -> event_span, INFO, "Process events");

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
        let object_bind_group_key = object_manager.ready(
            &renderer.device,
            &mut encoder,
            &material_manager,
            &renderer.global_resources.read().object_input_bgl,
        );

        drop((mesh_manager, texture_manager, material_manager, object_manager));

        span_transfer!(event_span -> resource_update_span, INFO, "Update resources");

        if let Some(ref new_opt) = new_options {
            renderer
                .global_resources
                .write()
                .update(&renderer.device, &renderer.surface, &renderer.options, new_opt);
        }
        let global_resources_guard = renderer.global_resources.read();
        global_resources_guard
            .uniforms
            .upload(&renderer.queue, &global_resources_guard.camera);

        let culling_pass_data = renderer.culling_pass.prepare(
            &renderer.device,
            &global_resources_guard.object_output_bgl,
            renderer.object_manager.read().object_count() as u32,
            String::from("primary render"),
        );

        span_transfer!(resource_update_span -> compute_pass_span, INFO, "Primary ComputePass");

        let object_manager = renderer.object_manager.read();

        let mut cpass = encoder.begin_compute_pass();
        renderer.culling_pass.run(
            &mut cpass,
            object_manager.bind_group(&object_bind_group_key),
            &global_resources_guard.uniforms.uniform_bg,
            &culling_pass_data,
        );
        drop(cpass);
        drop(global_resources_guard);

        span_transfer!(compute_pass_span -> render_pass_span, INFO, "Primary Renderpass");

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

        span_transfer!(render_pass_span -> queue_submit_span, INFO, "Submitting to Queue");

        renderer.queue.submit(std::iter::once(encoder.finish()));

        span_transfer!(queue_submit_span -> buffer_pump_span, INFO, "Pumping Buffers");

        let futures = renderer.buffer_manager.lock().pump();
        for future in futures {
            let span = tracing::debug_span!("Buffer recall");
            renderer
                .yard
                .spawn(COMPUTE_POOL, BUFFER_RECALL_PRIORITY, future.instrument(span));
        }

        span_transfer!(buffer_pump_span -> present_span, INFO, "Presenting");

        drop(frame); //

        span_transfer!(present_span -> drop_span, INFO, "Dropping loop data");

        RendererStatistics {}
    }
    .instrument(render_loop_span)
}
