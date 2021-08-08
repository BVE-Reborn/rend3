use crate::{
    datatypes::{Camera, CameraProjection},
    instruction::Instruction,
    statistics::RendererStatistics,
    util::output::RendererOutput,
    RenderRoutine, Renderer,
};
use std::{future::Future, sync::Arc};
use tracing_futures::Instrument;
use wgpu::{
    util::DeviceExt, CommandEncoderDescriptor, Extent3d, TextureAspect, TextureDescriptor, TextureDimension,
    TextureUsage, TextureViewDescriptor, TextureViewDimension,
};

pub fn render_loop<TLD: 'static>(
    renderer: Arc<Renderer<TLD>>,
    list: Arc<dyn RenderRoutine<TLD>>,
    output: RendererOutput,
) -> impl Future<Output = RendererStatistics> {
    span_transfer!(_ -> render_create_span, INFO, "Render Loop Creation");

    // blocks, do it before we async
    renderer.instructions.swap();

    let render_loop_span = tracing::warn_span!("Render Loop");
    async move {
        let mut instructions = renderer.instructions.consumer.lock();

        span_transfer!(_ -> event_span, INFO, "Process events");

        let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("primary encoder"),
        });

        let mut new_options = None;

        let mut mesh_manager = renderer.mesh_manager.write();
        let mut texture_manager_2d = renderer.d2_texture_manager.write();
        let mut texture_manager_cube = renderer.d2c_texture_manager.write();
        let mut material_manager = renderer.material_manager.write();
        let mut object_manager = renderer.object_manager.write();
        let mut directional_light_manager = renderer.directional_light_manager.write();
        let mut global_resources = renderer.global_resources.write();
        let mut option_guard = renderer.options.write();

        for cmd in instructions.drain(..) {
            match cmd {
                Instruction::AddMesh { handle, mesh } => {
                    mesh_manager.fill(&renderer.device, &renderer.queue, &mut encoder, handle, mesh);
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

                    assert!(texture.mip_levels > 0, "Mipmap levels must be greater than 0");

                    let uploaded_tex = renderer.device.create_texture_with_data(
                        &renderer.queue,
                        &TextureDescriptor {
                            label: None,
                            size,
                            mip_level_count: texture.mip_levels,
                            sample_count: 1,
                            dimension: TextureDimension::D2,
                            format: texture.format.into(),
                            usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
                        },
                        &texture.data,
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

                    assert!(texture.mip_levels > 0, "Mipmap levels must be greater than 0");

                    let uploaded_tex = renderer.device.create_texture_with_data(
                        &renderer.queue,
                        &TextureDescriptor {
                            label: None,
                            size,
                            mip_level_count: texture.mip_levels,
                            sample_count: 1,
                            dimension: TextureDimension::D2,
                            format: texture.format.into(),
                            usage: TextureUsage::SAMPLED | TextureUsage::COPY_DST,
                        },
                        &texture.data,
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
                        handle,
                        material,
                    );
                }
                Instruction::ChangeMaterial { handle, change } => {
                    material_manager.update_from_changes(&renderer.queue, handle, change);
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
                        value.camera.set_data(
                            Camera {
                                projection: CameraProjection::from_orthographic_direction(direction.into()),
                                ..Camera::default()
                            },
                            None,
                        );
                    }
                }
                Instruction::RemoveDirectionalLight { handle } => directional_light_manager.remove(handle),
                Instruction::SetOptions { options } => new_options = Some(options),
                Instruction::SetCameraData { data } => {
                    global_resources
                        .camera
                        .set_data(data, Some(option_guard.aspect_ratio()));
                }
                Instruction::SetBackgroundTexture { handle } => {
                    global_resources.background_texture = Some(handle);
                }
                Instruction::ClearBackgroundTexture => {
                    global_resources.background_texture = None;
                }
            }
        }

        let current_options = if let Some(new_opt) = new_options {
            global_resources.update(&renderer.device, renderer.surface.as_ref(), &mut *option_guard, new_opt);
            option_guard.clone()
        } else {
            renderer.options.read().clone()
        };

        drop((
            global_resources,
            option_guard,
            mesh_manager,
            texture_manager_2d,
            texture_manager_cube,
            material_manager,
            object_manager,
            directional_light_manager,
        ));

        let frame = output.acquire(&renderer.global_resources.read().swapchain);

        // 16 encoders is a reasonable default
        let mut encoders = Vec::with_capacity(16);
        encoders.push(encoder.finish());

        list.render(Arc::clone(&renderer), &mut encoders, frame);

        renderer.queue.submit(encoders);

        RendererStatistics {}
    }
    .instrument(render_loop_span)
}
