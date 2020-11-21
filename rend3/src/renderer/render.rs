use crate::{
    bind_merge::BindGroupBuilder,
    instruction::Instruction,
    list::{RenderList, RenderPassRunRate},
    renderer::{
        list, list::OutputFrame, uniforms::WrappedUniform, BUFFER_RECALL_PRIORITY, COMPUTE_POOL, RENDER_RECORD_PRIORITY,
    },
    statistics::RendererStatistics,
    Renderer,
};
use futures::{stream::FuturesOrdered, StreamExt};
use std::{borrow::Cow, future::Future, sync::Arc};
use tracing_futures::Instrument;
use wgpu::{
    BindGroupEntry, BindingResource, CommandEncoderDescriptor, Extent3d, Origin3d, ShaderModuleSource, SwapChainError,
    TextureAspect, TextureCopyView, TextureDataLayout, TextureDescriptor, TextureDimension, TextureUsage,
    TextureViewDescriptor, TextureViewDimension,
};

pub fn render_loop<TLD: 'static>(
    renderer: Arc<Renderer<TLD>>,
    render_list: RenderList,
) -> impl Future<Output = RendererStatistics> {
    span_transfer!(_ -> render_create_span, INFO, "Render Loop Creation");

    // blocks, do it before we async
    renderer.instructions.swap();

    let render_loop_span = tracing::warn_span!("Render Loop");
    async move {
        let mut instructions = renderer.instructions.consumer.lock();

        span_transfer!(_ -> event_span, INFO, "Process events");

        let (frame, mut command_buffer_futures, mut command_buffers) = {
            let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("primary encoder"),
            });

            let mut new_options = None;

            let mut mesh_manager = renderer.mesh_manager.write();
            let mut texture_manager_2d = renderer.texture_manager_2d.write();
            let mut texture_manager_cube = renderer.texture_manager_cube.write();
            let mut material_manager = renderer.material_manager.write();
            let mut object_manager = renderer.object_manager.write();
            let mut directional_light_manager = renderer.directional_light_manager.write();
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
                                bytes_per_row: texture.format.bytes_per_block()
                                    * (texture.width / texture.format.pixels_per_block()),
                                rows_per_image: 0,
                            },
                            size,
                        );

                        texture_manager_2d.fill(
                            handle,
                            uploaded_tex.create_view(&TextureViewDescriptor::default()),
                            Some(texture.format),
                        );
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

                        renderer.queue.write_texture(
                            TextureCopyView {
                                texture: &uploaded_tex,
                                origin: Origin3d { x: 0, y: 0, z: 0 },
                                mip_level: 0,
                            },
                            &texture.data,
                            TextureDataLayout {
                                offset: 0,
                                bytes_per_row: texture.format.bytes_per_block()
                                    * (texture.width / texture.format.pixels_per_block()),
                                rows_per_image: texture.height,
                            },
                            Extent3d {
                                width: texture.width,
                                height: texture.height,
                                depth: 6,
                            },
                        );

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
                            Some(texture.format),
                        );
                    }
                    Instruction::RemoveTextureCube { handle } => {
                        texture_manager_cube.remove(handle);
                    }
                    Instruction::AddMaterial { handle, material } => {
                        material_manager.fill(
                            &renderer.device,
                            renderer.mode,
                            &mut texture_manager_2d,
                            &global_resources.material_bgl,
                            handle,
                            material,
                        );
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
                    Instruction::AddDirectionalLight { handle, light } => {
                        directional_light_manager.fill(handle, light);
                    }
                    Instruction::ChangeDirectionalLight { handle, change } => {
                        // TODO: Move these inside the managers
                        let value = directional_light_manager.get_mut(handle);
                        value.inner.update_from_changes(change);
                        if let Some(direction) = change.direction {
                            value.camera.set_orthographic_location(direction);
                        }
                    }
                    Instruction::RemoveDirectionalLight { handle } => directional_light_manager.remove(handle),
                    Instruction::AddBinaryShader { handle, shader } => {
                        let module = renderer
                            .device
                            .create_shader_module(ShaderModuleSource::SpirV(Cow::Owned(shader)));
                        renderer.shader_manager.insert(handle, Arc::new(module));
                    }
                    Instruction::RemoveShader { handle } => {
                        renderer.shader_manager.remove(handle);
                    }
                    Instruction::RemovePipeline { handle } => {
                        renderer.pipeline_manager.remove(handle);
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

            renderer
                .render_list_cache
                .write()
                .add_render_list(&renderer.device, render_list.resources);

            let (texture_2d_bgl, texture_2d_bg, texture_2d_bgl_dirty) = texture_manager_2d.ready(&renderer.device);
            let (texture_cube_bgl, texture_cube_bg, texture_cube_bgl_dirty) =
                texture_manager_cube.ready(&renderer.device);
            material_manager.ready(&renderer.device, &mut encoder, &texture_manager_2d);
            let object_count = object_manager.ready(&renderer.device, &mut encoder, &material_manager);
            directional_light_manager.ready(&renderer.device, &mut encoder);

            let mut object_input_bgb = BindGroupBuilder::new(Some(String::from("object input bg")));
            object_manager.append_to_bgb(&mut object_input_bgb);
            let object_input_bg = object_input_bgb.build(&renderer.device, &global_resources.object_input_bgl);

            let mut general_bgb = BindGroupBuilder::new(Some(String::from("general bg")));
            global_resources.append_to_bgb(&mut general_bgb);
            let general_bg = general_bgb.build(&renderer.device, &global_resources.general_bgl);

            let mut material_bgb = BindGroupBuilder::new(Some(String::from("material bg")));
            material_manager.append_to_bgb(&mut material_bgb);
            let material_bg = material_bgb.build(&renderer.device, &global_resources.material_bgl);

            let mut shadow_bgb = BindGroupBuilder::new(Some(String::from("shadow bg")));
            directional_light_manager.append_to_bgb(&mut shadow_bgb);
            let shadow_bg = shadow_bgb.build(&renderer.device, &global_resources.shadow_texture_bgl);

            let skybox_texture_view = if let Some(ref sky) = global_resources.background_texture {
                texture_manager_cube.get_view(*sky)
            } else {
                texture_manager_cube.get_null_view()
            };
            let mut skybox_bgb = BindGroupBuilder::new(Some(String::from("skybox bg")));
            skybox_bgb.append(BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureView(skybox_texture_view),
            });
            let skybox_bg = skybox_bgb.build(&renderer.device, &global_resources.skybox_bgl);

            drop((
                mesh_manager,
                texture_manager_2d,
                texture_manager_cube,
                material_manager,
                object_manager,
                directional_light_manager,
            ));

            span_transfer!(event_span -> resource_update_span, INFO, "Update resources");

            if let Some(new_opt) = new_options {
                global_resources.update(
                    &renderer.device,
                    &renderer.surface,
                    &mut renderer.options.write(),
                    new_opt,
                );
            }

            drop(global_resources);

            let global_resources = renderer.global_resources.read();
            let directional_light_manager = renderer.directional_light_manager.read();

            let mut command_buffer_futures = FuturesOrdered::new();

            for light in directional_light_manager.values() {
                let cull_data = Arc::new(renderer.culling_pass.prepare(
                    &renderer.device,
                    &global_resources.prefix_sum_bgl,
                    &global_resources.pre_cull_bgl,
                    &global_resources.object_output_bgl,
                    object_count as _,
                    String::from("shadow pass"),
                ));

                let mut object_bgb = BindGroupBuilder::new(Some(String::from("object bg")));
                object_bgb.append(BindGroupEntry {
                    binding: 0,
                    resource: cull_data.output_buffer.as_entire_binding(),
                });
                let object_bg = object_bgb.build(&renderer.device, &global_resources.object_data_bgl);

                let uniform = WrappedUniform::new(&renderer.device, &global_resources.camera_data_bgl);
                uniform.upload(&renderer.queue, &light.camera);

                let mut cpass = encoder.begin_compute_pass();

                renderer
                    .culling_pass
                    .run(&mut cpass, &object_input_bg, &uniform.uniform_bg, &*cull_data);

                drop(cpass);

                let binding_data = list::BindingData {
                    general_bg: Arc::clone(&general_bg),
                    object_bg: Arc::clone(&object_bg),
                    material_bg: Arc::clone(&material_bg),
                    gpu_2d_textures_bg: Arc::clone(&texture_2d_bg),
                    gpu_cube_textures_bg: Arc::clone(&texture_cube_bg),
                    shadow_texture_bg: Arc::clone(&shadow_bg),
                    skybox_texture_bg: Arc::clone(&skybox_bg),
                    wrapped_uniform: Arc::new(uniform),
                };

                for render_pass in &render_list.passes {
                    if render_pass.desc.run_rate != RenderPassRunRate::PerShadow {
                        continue;
                    }

                    let output = directional_light_manager.get_layer_view_arc(light.shadow_tex);

                    command_buffer_futures.push(renderer.yard.spawn(
                        COMPUTE_POOL,
                        RENDER_RECORD_PRIORITY,
                        list::render_single_render_pass(
                            Arc::clone(&renderer),
                            render_pass.clone(),
                            OutputFrame::Shadow(output),
                            Arc::clone(&cull_data),
                            binding_data.clone(),
                        ),
                    ));
                }
            }

            drop(directional_light_manager);

            let mut frame = None;
            while frame.is_none() {
                match global_resources.swapchain.get_current_frame() {
                    Ok(v) => frame = Some(v),
                    Err(SwapChainError::Timeout) => {}
                    Err(err) => panic!("Could not make swapchain: {}", err),
                }
            }

            let frame = Arc::new(frame.unwrap());

            {
                let cull_data = Arc::new(renderer.culling_pass.prepare(
                    &renderer.device,
                    &global_resources.prefix_sum_bgl,
                    &global_resources.pre_cull_bgl,
                    &global_resources.object_output_bgl,
                    object_count as _,
                    String::from("camera pass"),
                ));

                let mut object_bgb = BindGroupBuilder::new(Some(String::from("object bg")));
                object_bgb.append(BindGroupEntry {
                    binding: 0,
                    resource: cull_data.output_buffer.as_entire_binding(),
                });
                let object_bg = object_bgb.build(&renderer.device, &global_resources.object_data_bgl);

                let uniform = WrappedUniform::new(&renderer.device, &global_resources.camera_data_bgl);
                uniform.upload(&renderer.queue, &global_resources.camera);

                let mut cpass = encoder.begin_compute_pass();

                renderer
                    .culling_pass
                    .run(&mut cpass, &object_input_bg, &uniform.uniform_bg, &*cull_data);

                drop(cpass);

                let binding_data = list::BindingData {
                    general_bg: Arc::clone(&general_bg),
                    object_bg: Arc::clone(&object_bg),
                    material_bg: Arc::clone(&material_bg),
                    gpu_2d_textures_bg: Arc::clone(&texture_2d_bg),
                    gpu_cube_textures_bg: Arc::clone(&texture_cube_bg),
                    shadow_texture_bg: Arc::clone(&shadow_bg),
                    skybox_texture_bg: Arc::clone(&skybox_bg),
                    wrapped_uniform: Arc::new(uniform),
                };

                for render_pass in &render_list.passes {
                    if render_pass.desc.run_rate != RenderPassRunRate::Once {
                        continue;
                    }

                    command_buffer_futures.push(renderer.yard.spawn(
                        COMPUTE_POOL,
                        RENDER_RECORD_PRIORITY,
                        list::render_single_render_pass(
                            Arc::clone(&renderer),
                            render_pass.clone(),
                            OutputFrame::Swapchain(Arc::clone(&frame)),
                            Arc::clone(&cull_data),
                            binding_data.clone(),
                        ),
                    ));
                }
            }

            drop(global_resources);

            span_transfer!(resource_update_span -> _);

            let command_buffers = vec![encoder.finish()];

            (frame, command_buffer_futures, command_buffers)
        };

        while let Some(buffer) = command_buffer_futures.next().await {
            command_buffers.push(buffer);
        }

        span_transfer!(_ -> queue_submit_span, INFO, "Submitting to Queue");

        renderer.queue.submit(command_buffers);

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
