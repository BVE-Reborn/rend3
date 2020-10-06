use crate::{
    instruction::Instruction,
    renderer::{BUFFER_RECALL_PRIORITY, COMPUTE_POOL},
    statistics::RendererStatistics,
    Renderer,
};
use std::{future::Future, sync::Arc};
use tracing_futures::Instrument;
use wgpu::{
    Color, CommandEncoderDescriptor, Extent3d, LoadOp, Operations, Origin3d, RenderPassColorAttachmentDescriptor,
    RenderPassDepthStencilAttachmentDescriptor, RenderPassDescriptor, TextureAspect, TextureCopyView,
    TextureDataLayout, TextureDescriptor, TextureDimension, TextureUsage, TextureViewDescriptor, TextureViewDimension,
};

pub fn render_loop<TLD: 'static>(renderer: Arc<Renderer<TLD>>) -> impl Future<Output = RendererStatistics> {
    span_transfer!(_ -> render_create_span, INFO, "Render Loop Creation");

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
        let mut texture_manager_2d = renderer.texture_manager_2d.write();
        let mut texture_manager_cube = renderer.texture_manager_cube.write();
        let mut material_manager = renderer.material_manager.write();
        let mut object_manager = renderer.object_manager.write();
        let mut global_resources = renderer.global_resources.write();

        for cmd in instructions.drain(..) {
            match cmd {
                Instruction::AddMesh { handle, mesh } => {
                    mesh_manager.fill(&renderer.queue, handle, mesh);
                }
                Instruction::RemoveMesh { handle } => {
                    mesh_manager.remove(handle);
                }
                Instruction::AddTexture2D { handle, texture } => {
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

                    texture_manager_2d.fill(handle, uploaded_tex.create_view(&TextureViewDescriptor::default()));
                }
                Instruction::RemoveTexture2D { handle } => {
                    texture_manager_2d.remove(handle);
                }
                Instruction::AddTextureCube { handle, texture } => {
                    let size = Extent3d {
                        width: texture.width,
                        height: texture.height,
                        depth: 6,
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

                    let bytes_per_image = (texture.width * texture.height * texture.format.bytes_per_pixel()) as usize;
                    for i in 0..6 {
                        renderer.queue.write_texture(
                            TextureCopyView {
                                texture: &uploaded_tex,
                                origin: Origin3d {
                                    x: 0,
                                    y: 0,
                                    z: i as u32,
                                },
                                mip_level: 0,
                            },
                            &texture.data[(i * bytes_per_image)..((i + 1) * bytes_per_image)],
                            TextureDataLayout {
                                offset: 0,
                                bytes_per_row: texture.format.bytes_per_pixel() * texture.width,
                                rows_per_image: 0,
                            },
                            Extent3d {
                                width: texture.width,
                                height: texture.height,
                                depth: 1,
                            },
                        );
                    }

                    texture_manager_cube.fill(
                        handle,
                        uploaded_tex.create_view(&TextureViewDescriptor {
                            label: None,
                            format: Some(texture.format.into()),
                            dimension: Some(TextureViewDimension::Cube),
                            aspect: TextureAspect::All,
                            base_mip_level: 0,
                            level_count: None,
                            base_array_layer: 0,
                            array_layer_count: None,
                        }),
                    );
                }
                Instruction::RemoveTextureCube { handle } => {
                    texture_manager_cube.remove(handle);
                }
                Instruction::AddMaterial { handle, material } => {
                    material_manager.fill(handle, material);
                }
                Instruction::ChangeMaterial { handle, change } => {
                    material_manager.get_mut(handle).update_from_changes(change)
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
                Instruction::SetCameraLocation { location } => {
                    global_resources.camera.set_location(location);
                }
                Instruction::SetBackgroundTexture { handle } => {
                    global_resources.background_texture = Some(handle);
                }
                Instruction::ClearBackgroundTexture => {
                    global_resources.background_texture = None;
                }
            }
        }

        let mut general_bgm = renderer.general_bgm.lock();
        let mut general_bgb = general_bgm.builder();

        let (texture_2d_bgl, texture_2d_bg, texture_2d_bgl_dirty) =
            texture_manager_2d.ready(&renderer.device, &global_resources.sampler);
        let (texture_cube_bgl, texture_cube_bg, texture_cube_bgl_dirty) =
            texture_manager_cube.ready(&renderer.device, &global_resources.sampler);
        material_manager.ready(&renderer.device, &mut encoder, &texture_manager_2d);
        let object_count = object_manager.ready(&renderer.device, &mut encoder, &material_manager);

        object_manager.append_to_bgb(&mut general_bgb);
        material_manager.append_to_bgb(&mut general_bgb);

        let (general_bgl, general_bg, general_bgl_dirty) = general_bgb.build(&renderer.device);

        drop((
            general_bgm,
            global_resources,
            mesh_manager,
            texture_manager_2d,
            texture_manager_cube,
            material_manager,
            object_manager,
        ));

        span_transfer!(event_span -> resource_update_span, INFO, "Update resources");

        if let Some(new_opt) = new_options {
            renderer.global_resources.write().update(
                &renderer.device,
                &renderer.surface,
                &mut renderer.options.write(),
                new_opt,
            );
        }

        let global_resources = renderer.global_resources.read();

        let forward_pass_data =
            renderer
                .forward_pass_set
                .prepare(&renderer, &global_resources, &global_resources.camera, object_count);

        if texture_2d_bgl_dirty || general_bgl_dirty {
            renderer.depth_pass.write().update_pipeline(
                &renderer.device,
                &general_bgl,
                &global_resources.object_output_noindirect_bgl,
                &texture_2d_bgl,
                &global_resources.uniform_bgl,
            );
            renderer.opaque_pass.write().update_pipeline(
                &renderer.device,
                &general_bgl,
                &global_resources.object_output_noindirect_bgl,
                &texture_2d_bgl,
                &global_resources.uniform_bgl,
            );
        }

        if texture_cube_bgl_dirty {
            renderer.skybox_pass.write().update_pipeline(
                &renderer.device,
                &texture_cube_bgl,
                &global_resources.uniform_bgl,
            );
        }

        span_transfer!(resource_update_span -> compute_pass_span, INFO, "Primary ComputePass");

        let mesh_manager = renderer.mesh_manager.read();
        let (vertex_buffer, index_buffer) = mesh_manager.buffers();
        let object_manager = renderer.object_manager.read();

        let mut cpass = encoder.begin_compute_pass();
        renderer
            .forward_pass_set
            .compute(&renderer.culling_pass, &mut cpass, &general_bg, &forward_pass_data);
        drop(cpass);

        span_transfer!(compute_pass_span -> render_pass_span, INFO, "Primary Renderpass");

        drop(global_resources);

        let frame = renderer.global_resources.write().swapchain.get_current_frame().unwrap();

        let global_resources = renderer.global_resources.read();
        let texture_manager_cube = renderer.texture_manager_cube.read();
        let material_manager = renderer.material_manager.read();
        let depth_pass = renderer.depth_pass.read();
        let skybox_pass = renderer.skybox_pass.read();
        let opaque_pass = renderer.opaque_pass.read();

        let background_texture = global_resources
            .background_texture
            .map(|handle| texture_manager_cube.internal_index(handle) as u32);

        drop(texture_manager_cube);

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
            color_attachments: &[
                RenderPassColorAttachmentDescriptor {
                    attachment: &global_resources.color_texture_view,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: true,
                    },
                    resolve_target: None,
                },
                RenderPassColorAttachmentDescriptor {
                    attachment: &global_resources.normal_texture_view,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: true,
                    },
                    resolve_target: None,
                },
            ],
            depth_stencil_attachment: Some(RenderPassDepthStencilAttachmentDescriptor {
                attachment: &global_resources.depth_texture_view,
                depth_ops: Some(Operations {
                    load: LoadOp::Clear(0.0),
                    store: true,
                }),
                stencil_ops: None,
            }),
        });

        renderer.forward_pass_set.render(
            &depth_pass,
            &skybox_pass,
            &opaque_pass,
            &mut rpass,
            vertex_buffer,
            index_buffer,
            &general_bg,
            &texture_2d_bg,
            &texture_cube_bg,
            &forward_pass_data,
            background_texture,
        );

        drop(rpass);

        drop((opaque_pass, depth_pass, object_manager, material_manager, mesh_manager));

        span_transfer!(render_pass_span -> blit_span, INFO, "Blit to Swapchain");

        let mut rpass = encoder.begin_render_pass(&RenderPassDescriptor {
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

        renderer.swapchain_blit_pass.run(&mut rpass, &global_resources.color_bg);

        drop(rpass);

        drop(global_resources);

        span_transfer!(blit_span -> queue_submit_span, INFO, "Submitting to Queue");

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
