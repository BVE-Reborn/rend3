use std::sync::Arc;

fn load_gltf(
    renderer: &rend3::Renderer,
    path: &'static str,
) -> (rend3::datatypes::MeshHandle, rend3::datatypes::MaterialHandle) {
    let (doc, datas, _) = gltf::import(path).unwrap();
    let mesh_data = doc.meshes().next().expect("no meshes in data.glb");

    let primitive = mesh_data.primitives().next().expect("no primitives in data.glb");
    let reader = primitive.reader(|b| Some(&datas.get(0)?.0[b.index()..b.length()]));
    let mut normals = reader.read_normals();
    let mut uv = reader.read_tex_coords(0).map(|tc| tc.into_f32());
    let mut mesh = rend3::datatypes::Mesh {
        vertices: reader
            .read_positions()
            .unwrap()
            .map(|pos| rend3::datatypes::ModelVertex {
                position: pos.into(),
                normal: normals.as_mut().and_then(|n| n.next()).unwrap_or_default().into(),
                uv: uv.as_mut().and_then(|n| n.next()).unwrap_or_default().into(),
                color: [0; 4],
            })
            .collect(),
        indices: reader.read_indices().unwrap().into_u32().collect(),
    };
    if normals.is_none() {
        mesh.calculate_normals();
    }

    // Add mesh to renderer's world
    let mesh_handle = renderer.add_mesh(mesh);

    // Add basic material with all defaults except a single color.
    let material = rend3::datatypes::Material {
        albedo: rend3::datatypes::AlbedoComponent::Value(
            primitive.material().pbr_metallic_roughness().base_color_factor().into(),
        ),
        ..Default::default()
    };
    let material_handle = renderer.add_material(material);

    (mesh_handle, material_handle)
}

fn main() {
    // Setup logging
    wgpu_subscriber::initialize_default_subscriber(None);

    // Create event loop and window
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let mut builder = winit::window::WindowBuilder::new();
        builder = builder.with_title("rend3 gltf");
        builder.build(&event_loop).expect("Could not build window")
    };

    // Create executor needed by rend3
    let yard = Arc::new(
        switchyard::Switchyard::new(
            2,
            switchyard::threads::double_pool_one_to_one(switchyard::threads::thread_info(), Some("rend3 gltf")),
            || (),
        )
        .unwrap(),
    );

    let window_size = window.inner_size();

    let mut options = rend3::RendererOptions {
        vsync: rend3::VSyncMode::On,
        size: [window_size.width, window_size.height],
    };

    let renderer = pollster::block_on(rend3::Renderer::new(
        &window,         //
        yard,            //
        None,            // No backend preference
        None,            // No device name preference
        None,            // No rendering mode preference
        options.clone(), //
    ))
    .unwrap();

    // Create the default set of shaders and pipelines
    let pipelines = pollster::block_on(async {
        let shaders = rend3::list::DefaultShaders::new(&renderer).await;
        rend3::list::DefaultPipelines::new(&renderer, &shaders).await
    });

    // Create mesh and calculate smooth normals based on vertices
    let (mesh, material) = load_gltf(&renderer, concat!(env!("CARGO_MANIFEST_DIR"), "/data.glb"));

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::datatypes::Object {
        mesh,
        material,
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
            let handle = renderer.render(render_list);

            // Wait until it's done
            pollster::block_on(handle);
        }
        // Other events we don't care about
        _ => {}
    });
}
