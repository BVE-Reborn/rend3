use crate::{
    instruction::Instruction,
    util::{output::RendererOutput, typedefs::RendererStatistics},
    RenderRoutine, Renderer,
};
use rend3_types::{MipmapCount, MipmapSource};
use std::{num::NonZeroU32, sync::Arc};
use wgpu::{
    util::DeviceExt, CommandEncoderDescriptor, Extent3d, ImageCopyTexture, ImageDataLayout, Origin3d, TextureAspect,
    TextureDescriptor, TextureDimension, TextureUsages, TextureViewDescriptor, TextureViewDimension,
};

pub fn render_loop(
    renderer: Arc<Renderer>,
    routine: &mut dyn RenderRoutine,
    output: RendererOutput,
) -> Option<RendererStatistics> {
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
    let mut mipmap_generator = renderer.mipmap_generator.lock();
    {
        profiling::scope!("Instruction Processing");
        for cmd in instructions.drain(..) {
            match cmd {
                Instruction::AddMesh { handle, mesh } => {
                    profiling::scope!("Add Mesh");
                    renderer
                        .profiler
                        .lock()
                        .begin_scope("Add Mesh", &mut encoder, &renderer.device);
                    mesh_manager.fill(&renderer.device, &renderer.queue, &mut encoder, &handle, mesh);
                    renderer.profiler.lock().end_scope(&mut encoder);
                }
                Instruction::AddTexture2D { handle, texture } => {
                    profiling::scope!("Add Texture 2D");
                    let size = Extent3d {
                        width: texture.size.x,
                        height: texture.size.y,
                        depth_or_array_layers: 1,
                    };

                    let mip_level_count = match texture.mip_count {
                        MipmapCount::Specific(v) => v.get(),
                        MipmapCount::Maximum => size.max_mips(),
                    };

                    let uploaded_tex = match texture.mip_source {
                        MipmapSource::Uploaded => renderer.device.create_texture_with_data(
                            &renderer.queue,
                            &TextureDescriptor {
                                label: None,
                                size,
                                mip_level_count,
                                sample_count: 1,
                                dimension: TextureDimension::D2,
                                format: texture.format,
                                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
                            },
                            &texture.data,
                        ),
                        MipmapSource::Generated => {
                            let desc = TextureDescriptor {
                                label: None,
                                size,
                                mip_level_count,
                                sample_count: 1,
                                dimension: TextureDimension::D2,
                                format: texture.format,
                                usage: TextureUsages::TEXTURE_BINDING
                                    | TextureUsages::COPY_DST
                                    | TextureUsages::RENDER_ATTACHMENT,
                            };
                            let tex = renderer.device.create_texture(&desc);

                            let format_desc = texture.format.describe();

                            // write first level
                            renderer.queue.write_texture(
                                ImageCopyTexture {
                                    texture: &tex,
                                    mip_level: 0,
                                    origin: Origin3d::ZERO,
                                    aspect: TextureAspect::All,
                                },
                                &texture.data,
                                ImageDataLayout {
                                    offset: 0,
                                    bytes_per_row: NonZeroU32::new(
                                        format_desc.block_size as u32 * (size.width / format_desc.block_dimensions.0 as u32),
                                    ),
                                    rows_per_image: None,
                                },
                                size,
                            );

                            // generate mipmaps
                            mipmap_generator.generate_mipmaps(
                                &renderer.device,
                                &mut renderer.profiler.lock(),
                                &mut encoder,
                                &tex,
                                &desc,
                            );

                            tex
                        }
                    };

                    texture_manager_2d.fill(
                        &handle,
                        uploaded_tex.create_view(&TextureViewDescriptor::default()),
                        Some(texture.format),
                    );
                }
                Instruction::AddTextureCube { handle, texture } => {
                    profiling::scope!("Add Texture Cube");
                    let size = Extent3d {
                        width: texture.size.x,
                        height: texture.size.y,
                        depth_or_array_layers: 6,
                    };

                    let mip_level_count = match texture.mip_count {
                        MipmapCount::Specific(v) => v.get(),
                        MipmapCount::Maximum => size.max_mips(),
                    };

                    let uploaded_tex = renderer.device.create_texture_with_data(
                        &renderer.queue,
                        &TextureDescriptor {
                            label: None,
                            size,
                            mip_level_count,
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
                    profiling::scope!("Add Texture Material");
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

    let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("resolve encoder"),
    });
    renderer.profiler.lock().resolve_queries(&mut encoder);
    encoders.push(encoder.finish());

    renderer.queue.submit(encoders);

    let mut profiler = renderer.profiler.lock();
    profiler.end_frame().unwrap();
    profiler.process_finished_frame()
}
