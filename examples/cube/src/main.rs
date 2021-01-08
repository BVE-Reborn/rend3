fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn create_mesh() -> rend3::datatypes::Mesh {
    let vertex_positions = [
        // top (0.0, 0.0, 1.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        // bottom (0.0, 0.0, -1.0)
        vertex([-1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // right (1.0, 0.0, 0.0)
        vertex([1.0, -1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        // left (-1.0, 0.0, 0.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // front (0.0, 1.0, 0.0)
        vertex([1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        // back (0.0, -1.0, 0.0)
        vertex([1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
    ];

    let index_data: &[u32] = &[
        2, 1, 0, 0, 3, 2, // top
        6, 5, 4, 4, 7, 6, // bottom
        10, 9, 8, 8, 11, 10, // right
        14, 13, 12, 12, 15, 14, // left
        18, 17, 16, 16, 19, 18, // front
        22, 21, 20, 20, 23, 22, // back
    ];

    let count = vertex_positions.len();

    let mut mesh = rend3::datatypes::Mesh {
        vertex_positions: vertex_positions.to_vec(),
        vertex_normals: vec![glam::Vec3::zero(); count],
        vertex_uvs: vec![glam::Vec2::zero(); count],
        vertex_colors: vec![[0; 4]; count],
        vertex_material_indices: vec![0; count],
        indices: index_data.to_vec(),
    };

    // Auto-calculate normals for us
    mesh.calculate_normals();

    mesh
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
    };

    let renderer = pollster::block_on(rend3::RendererBuilder::new(options.clone()).window(&window).build()).unwrap();

    // Create the default set of shaders and pipelines
    let pipelines = pollster::block_on(async {
        let shaders = rend3::list::DefaultShaders::new(&renderer).await;
        rend3::list::DefaultPipelines::new(&renderer, &shaders).await
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
            transform: glam::Mat4::identity(),
        },
    };
    let _object_handle = renderer.add_object(object);

    // Set camera's location
    renderer.set_camera_location(rend3::datatypes::CameraLocation {
        location: glam::Vec3A::new(3.0, 3.0, -5.0),
        pitch: 0.5,
        yaw: -0.55,
    });

    // Create a single directional light
    renderer.add_directional_light(rend3::datatypes::DirectionalLight {
        color: glam::Vec3::one(),
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
            let render_list = rend3::list::default_render_list(renderer.mode(), internal_renderbuffer_size, &pipelines);

            // Dispatch a render!
            let handle = renderer.render(render_list, rend3::RendererOutput::InternalSwapchain);

            // Wait until it's done
            pollster::block_on(handle);
        }
        // Other events we don't care about
        _ => {}
    });
}
