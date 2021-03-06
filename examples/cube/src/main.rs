fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn create_mesh() -> rend3::datatypes::Mesh {
    let vertex_positions = [
        // far side (0.0, 0.0, 1.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        // near side (0.0, 0.0, -1.0)
        vertex([-1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // right side (1.0, 0.0, 0.0)
        vertex([1.0, -1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        // left side (-1.0, 0.0, 0.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // top (0.0, 1.0, 0.0)
        vertex([1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        // bottom (0.0, -1.0, 0.0)
        vertex([1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
    ];

    let index_data: &[u32] = &[
        0, 1, 2, 2, 3, 0, // far
        4, 5, 6, 6, 7, 4, // near
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // top
        20, 21, 22, 22, 23, 20, // bottom
    ];

    rend3::datatypes::MeshBuilder::new(vertex_positions.to_vec())
        .with_indices(index_data.to_vec())
        .build()
}

fn main() {
    // Setup logging
    wgpu_subscriber::initialize_default_subscriber(None);

    // Create event loop and window
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let mut builder = winit::window::WindowBuilder::new();
        builder = builder.with_title("rend3 cube");
        builder.build(&event_loop).expect("Could not build window")
    };

    let window_size = window.inner_size();

    let mut options = rend3::RendererOptions {
        vsync: rend3::VSyncMode::On,
        size: [window_size.width, window_size.height],
        ambient: glam::Vec4::default(),
    };

    let renderer = pollster::block_on(rend3::RendererBuilder::new(options.clone()).window(&window).build()).unwrap();

    // Create the default set of shaders and pipelines
    let pipelines = pollster::block_on(async {
        let shaders = rend3_list::DefaultShaders::new(&renderer).await;
        rend3_list::DefaultPipelines::new(&renderer, &shaders).await
    });

    // Create mesh and calculate smooth normals based on vertices
    let mesh = create_mesh();

    // Add mesh to renderer's world
    let mesh_handle = renderer.add_mesh(mesh);

    // Add basic material with all defaults except a single color.
    let material = rend3::datatypes::Material {
        albedo: rend3::datatypes::AlbedoComponent::Value(glam::Vec4::new(0.0, 0.5, 0.5, 1.0)),
        ..rend3::datatypes::Material::default()
    };
    let material_handle = renderer.add_material(material);

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::datatypes::Object {
        mesh: mesh_handle,
        material: material_handle,
        transform: rend3::datatypes::AffineTransform {
            transform: glam::Mat4::IDENTITY,
        },
    };
    let _object_handle = renderer.add_object(object);

    // Set camera's location
    renderer.set_camera_data(rend3::datatypes::Camera {
        projection: rend3::datatypes::CameraProjection::Projection {
            vfov: 60.0,
            near: 0.1,
            pitch: 0.5,
            yaw: -0.55,
        },
        location: glam::Vec3A::new(3.0, 3.0, -5.0),
    });

    // Create a single directional light
    renderer.add_directional_light(rend3::datatypes::DirectionalLight {
        color: glam::Vec3::ONE,
        intensity: 10.0,
        // Direction will be normalized
        direction: glam::Vec3::new(-1.0, -4.0, 2.0),
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
            options.size = [size.width, size.height];
            renderer.set_options(options.clone());
        }
        // Render!
        winit::event::Event::MainEventsCleared => {
            // Size of the internal buffers used for rendering.
            //
            // This can be different from the size of the swapchain,
            // it will be scaled to the swapchain size when being
            // rendered onto the swapchain.
            let internal_renderbuffer_size = options.size;

            // Default set of rendering commands using the default shaders.
            let render_list = rend3_list::default_render_list(renderer.mode(), internal_renderbuffer_size, &pipelines);

            // Dispatch a render!
            let handle = renderer.render(render_list, rend3::RendererOutput::InternalSwapchain);

            // Wait until it's done
            pollster::block_on(handle);
        }
        // Other events we don't care about
        _ => {}
    });
}
