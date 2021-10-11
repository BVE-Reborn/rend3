use image::GenericImageView;

fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn uv(pos: [f32; 2]) -> glam::Vec2 {
    glam::Vec2::from(pos)
}

fn create_quad(size: f32) -> rend3::types::Mesh {
    let vertex_positions = [
        vertex([-size * 0.5, size * 0.5, 0.0]),
        vertex([size * 0.5, size * 0.5, 0.0]),
        vertex([size * 0.5, -size * 0.5, 0.0]),
        vertex([-size * 0.5, -size * 0.5, 0.0]),
    ];
    let uv_positions = [uv([0.0, 0.0]), uv([1.0, 0.0]), uv([1.0, 1.0]), uv([0.0, 1.0])];
    let index_data: &[u32] = &[0, 1, 2, 2, 3, 0];

    rend3::types::MeshBuilder::new(vertex_positions.to_vec())
        .with_vertex_uv0(uv_positions.to_vec())
        .with_indices(index_data.to_vec())
        .build()
}

fn main() {
    // Setup logging
    env_logger::init();

    // Create event loop and window
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let mut builder = winit::window::WindowBuilder::new();
        builder = builder.with_title("rend3 textured quad");
        builder.build(&event_loop).expect("Could not build window")
    };

    let window_size = window.inner_size();

    // Create the Instance, Adapter, and Device. We can specify preferred backend, device name, or rendering mode. In this case we let rend3 choose for us.
    let iad = pollster::block_on(rend3::create_iad(None, None, None)).unwrap();

    // The one line of unsafe needed. We just need to guarentee that the window outlives the use of the surface.
    let surface = unsafe { iad.instance.create_surface(&window) };
    // Get the preferred format for the surface.
    let format = surface.get_preferred_format(&iad.adapter).unwrap();
    // Configure the surface to be ready for rendering.
    rend3::configure_surface(
        &surface,
        &iad.device,
        format,
        glam::UVec2::new(window_size.width, window_size.height),
        rend3::types::PresentMode::Mailbox,
    );

    // Make us a renderer.
    let renderer = rend3::Renderer::new(iad, Some(window_size.width as f32 / window_size.height as f32)).unwrap();

    // Create the pbr pipeline with the same internal resolution and 4x multisampling
    let mut routine = rend3_pbr::PbrRenderRoutine::new(
        &renderer,
        rend3_pbr::RenderTextureOptions {
            resolution: glam::UVec2::new(window_size.width, window_size.height),
            samples: rend3_pbr::SampleCount::Four,
        },
        format,
    );

    // Create mesh and calculate smooth normals based on vertices
    let mesh = create_quad(300.0);

    // Add mesh to renderer's world.
    //
    // All handles are refcounted, so we only need to hang onto the handle until we make an object.
    let mesh_handle = renderer.add_mesh(mesh);

    // Add texture to renderer's world.
    let image_checker =
        image::load_from_memory(include_bytes!("checker.png")).expect("Failed to load image from memory");
    let image_checker_rgba8 = image_checker.to_rgba8();
    let texture_checker = rend3::types::Texture {
        label: Option::None,
        data: image_checker_rgba8.to_vec(),
        format: rend3::types::TextureFormat::Rgba8UnormSrgb,
        size: glam::UVec2::new(image_checker.dimensions().0, image_checker.dimensions().1),
        mip_count: rend3::types::MipmapCount::ONE,
        mip_source: rend3::types::MipmapSource::Uploaded,
    };
    let texture_handle = renderer.add_texture_2d(texture_checker);

    // Add PBR material with all defaults except a single color.
    let material = rend3_pbr::material::PbrMaterial {
        albedo: rend3_pbr::material::AlbedoComponent::TextureValue {
            texture: texture_handle,
            value: glam::Vec4::new(1.0, 1.0, 1.0, 1.0),
        },
        unlit: true,
        ..rend3_pbr::material::PbrMaterial::default()
    };
    let material_handle = renderer.add_material(material);

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::types::Object {
        mesh: mesh_handle,
        material: material_handle,
        transform: glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::new(1.0, 1.0, 1.0),
            glam::Quat::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0),
            glam::Vec3::new(0.0, 0.0, 0.0),
        ),
    };

    // Creating an object will hold onto both the mesh and the material
    // even if they are deleted.
    //
    // We need to keep the object handle alive.
    let _object_handle = renderer.add_object(object);

    let view_location = glam::Vec3::new(0.0, 0.0, -1.0);
    let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0);
    let view = view * glam::Mat4::from_translation(-view_location);

    // Set camera's location
    const CAMERA_DEPTH: f32 = 10.0;
    renderer.set_camera_data(rend3::types::Camera {
        projection: rend3::types::CameraProjection::Orthographic {
            size: glam::Vec3A::new(
                window.inner_size().width as f32,
                window.inner_size().height as f32,
                CAMERA_DEPTH,
            ),
        },
        view,
    });

    event_loop.run(move |event, _, control| match event {
        // Close button was clicked, we should close.
        winit::event::Event::WindowEvent {
            event: winit::event::WindowEvent::CloseRequested,
            ..
        } => {
            *control = winit::event_loop::ControlFlow::Exit;
        }
        // Window was resized, need to resize renderer.
        winit::event::Event::WindowEvent {
            event: winit::event::WindowEvent::Resized(size),
            ..
        } => {
            let size = glam::UVec2::new(size.width, size.height);
            // Reconfigure the surface for the new size.
            rend3::configure_surface(
                &surface,
                &renderer.device,
                format,
                glam::UVec2::new(size.x, size.y),
                rend3::types::PresentMode::Mailbox,
            );
            // Tell the renderer about the new aspect ratio.
            renderer.set_aspect_ratio(size.x as f32 / size.y as f32);
            // Resize the internal buffers to the same size as the screen.
            routine.resize(
                &renderer,
                rend3_pbr::RenderTextureOptions {
                    resolution: size,
                    samples: rend3_pbr::SampleCount::One,
                },
            );
            // Reset camera
            renderer.set_camera_data(rend3::types::Camera {
                projection: rend3::types::CameraProjection::Orthographic {
                    size: glam::Vec3A::new(size.x as f32, size.y as f32, CAMERA_DEPTH),
                },
                view,
            });
        }
        // Render!
        winit::event::Event::MainEventsCleared => {
            // Get a frame
            let frame = rend3::util::output::OutputFrame::from_surface(&surface).unwrap();
            // Dispatch a render!
            let _stats = renderer.render(&mut routine, (), frame.as_view());
            // Present the frame on screen
            frame.present();
        }
        // Other events we don't care about
        _ => {}
    });
}
