use glam::UVec2;

fn load_gltf(
    renderer: &rend3::Renderer,
    path: &'static str,
) -> (rend3::types::MeshHandle, rend3::types::MaterialHandle) {
    let (doc, datas, _) = gltf::import(path).unwrap();
    let mesh_data = doc.meshes().next().expect("no meshes in data.glb");

    let primitive = mesh_data.primitives().next().expect("no primitives in data.glb");
    let reader = primitive.reader(|b| Some(&datas.get(b.index())?.0[..b.length()]));

    let vertex_positions: Vec<_> = reader.read_positions().unwrap().map(glam::Vec3::from).collect();
    let vertex_normals: Vec<_> = reader.read_normals().unwrap().map(glam::Vec3::from).collect();
    let vertex_tangents: Vec<_> = reader
        .read_tangents()
        .unwrap()
        .map(glam::Vec4::from)
        .map(From::from)
        .collect();
    let vertex_uvs: Vec<_> = reader
        .read_tex_coords(0)
        .unwrap()
        .into_f32()
        .map(glam::Vec2::from)
        .collect();
    let indices = reader.read_indices().unwrap().into_u32().collect();

    let mesh = rend3::types::MeshBuilder::new(vertex_positions.to_vec())
        .with_vertex_normals(vertex_normals)
        .with_vertex_tangents(vertex_tangents)
        .with_vertex_uvs(vertex_uvs)
        .with_indices(indices)
        .with_right_handed()
        .build();

    // Add mesh to renderer's world
    let mesh_handle = renderer.add_mesh(mesh);

    // Add basic material with all defaults except a single color.
    let material = primitive.material();
    let metallic_roughness = material.pbr_metallic_roughness();
    let material_handle = renderer.add_material(rend3::types::Material {
        albedo: rend3::types::AlbedoComponent::Value(metallic_roughness.base_color_factor().into()),
        ..Default::default()
    });

    (mesh_handle, material_handle)
}

fn main() {
    // Setup logging
    env_logger::init();

    // Create event loop and window
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let mut builder = winit::window::WindowBuilder::new();
        builder = builder.with_title("rend3 gltf");
        builder.build(&event_loop).expect("Could not build window")
    };

    let window_size = window.inner_size();

    let mut options = rend3::InternalSurfaceOptions {
        vsync: rend3::VSyncMode::On,
        size: UVec2::new(window_size.width, window_size.height),
    };

    let renderer = pollster::block_on(rend3::RendererBuilder::new(options.clone()).window(&window).build()).unwrap();

    // Create the default set of shaders and pipelines
    let mut routine = rend3_pbr::PbrRenderRoutine::new(&renderer, UVec2::new(window_size.width, window_size.height));

    // Create mesh and calculate smooth normals based on vertices
    let (mesh, material) = load_gltf(&renderer, concat!(env!("CARGO_MANIFEST_DIR"), "/data.glb"));

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::types::Object {
        mesh,
        material,
        transform: glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, -1.0)),
    };
    let _object_handle = renderer.add_object(object);

    // Set camera's location
    renderer.set_camera_data(rend3::types::Camera {
        projection: rend3::types::CameraProjection::Projection {
            vfov: 60.0,
            near: 0.1,
            pitch: 0.43,
            yaw: -0.49,
        },
        location: glam::Vec3A::new(3.0, 3.0, -5.0),
    });

    // Create a single directional light
    renderer.add_directional_light(rend3::types::DirectionalLight {
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
            options.size = UVec2::new(window_size.width, window_size.height);
            renderer.set_internal_surface_options(options.clone());
            routine.resize(&renderer.device, UVec2::new(size.width, size.height))
        }
        // Render!
        winit::event::Event::MainEventsCleared => {
            // Dispatch a render!
            let dynref: &mut dyn rend3::RenderRoutine = &mut routine;
            let _stats = renderer.render(dynref, rend3::util::output::RendererOutput::InternalSurface);
        }
        // Other events we don't care about
        _ => {}
    });
}
