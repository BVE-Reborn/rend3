use crate::{
    instruction::Instruction, statistics::RendererStatistics, util::output::RendererOutput, RenderRoutine, Renderer,
};
use std::sync::Arc;
use wgpu::{
    util::DeviceExt, CommandEncoderDescriptor, Extent3d, TextureDescriptor, TextureDimension, TextureUsages,
    TextureViewDescriptor, TextureViewDimension,
};

pub fn render_loop(
    renderer: Arc<Renderer>,
    routine: &mut dyn RenderRoutine,
    output: RendererOutput,
) -> RendererStatistics {
    profiling::scope!("render_loop");

    renderer.instructions.swap();

    let mut instructions = renderer.instructions.consumer.lock();

    let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("primary encoder"),
    });

    let mut new_surface_options = None;

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
                mesh_manager.fill(&renderer.device, &renderer.queue, &mut encoder, &handle, mesh);
            }
            Instruction::AddTexture2D { handle, texture } => {
                let size = Extent3d {
                    width: texture.size.x,
                    height: texture.size.y,
                    depth_or_array_layers: 1,
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
                        format: texture.format,
                        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                    },
                    &texture.data,
                );

                texture_manager_2d.fill(
                    &handle,
                    uploaded_tex.create_view(&TextureViewDescriptor::default()),
                    Some(texture.format),
                );
            }
            Instruction::AddTextureCube { handle, texture } => {
                let size = Extent3d {
                    width: texture.size.x,
                    height: texture.size.y,
                    depth_or_array_layers: 6,
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
                        format: texture.format,
                        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                    },
                    &texture.data,
                );

                texture_manager_cube.fill(
                    &handle,
                    uploaded_tex.create_view(&TextureViewDescriptor {
                        dimension: Some(TextureViewDimension::Cube),
                        ..TextureViewDescriptor::default()
                    }),
                    Some(texture.format),
                );
            }
            Instruction::AddMaterial { handle, material } => {
                material_manager.fill(
                    &renderer.device,
                    renderer.mode,
                    &mut texture_manager_2d,
                    &handle,
                    material,
                );
            }
            Instruction::ChangeMaterial { handle, change } => {
                material_manager.update_from_changes(&renderer.queue, handle, change);
            }
            Instruction::AddObject { handle, object } => {
                object_manager.fill(&handle, object, &mesh_manager);
            }
            Instruction::SetObjectTransform { handle, transform } => {
                object_manager.set_object_transform(handle, transform);
            }
            Instruction::AddDirectionalLight { handle, light } => {
                directional_light_manager.fill(&handle, light);
            }
            Instruction::ChangeDirectionalLight { handle, change } => {
                directional_light_manager.update_directional_light(handle, change);
            }
            Instruction::SetInternalSurfaceOptions { options } => new_surface_options = Some(options),
            Instruction::SetCameraData { data } => {
                global_resources
                    .camera
                    .set_data(data, Some(option_guard.aspect_ratio()));
            }
        }
    }

    if let Some(new_opt) = new_surface_options {
        global_resources.update(&renderer.device, renderer.surface.as_ref(), &mut *option_guard, new_opt);
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

    let frame = output.acquire(&renderer.surface);

    // 16 encoders is a reasonable default
    let mut encoders = Vec::with_capacity(16);
    encoders.push(encoder.finish());

    routine.render(Arc::clone(&renderer), &mut encoders, &frame);

    renderer.queue.submit(encoders);

    RendererStatistics {}
}
