use std::sync::Arc;

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
        .map(glam::Vec4::truncate)
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
        .with_vertex_uv0(vertex_uvs)
        .with_indices(indices)
        .with_right_handed()
        .build();

    // Add mesh to renderer's world
    let mesh_handle = renderer.add_mesh(mesh);

    // Add basic material with all defaults except a single color.
    let material = primitive.material();
    let metallic_roughness = material.pbr_metallic_roughness();
    let material_handle = renderer.add_material(rend3_pbr::material::PbrMaterial {
        albedo: rend3_pbr::material::AlbedoComponent::Value(metallic_roughness.base_color_factor().into()),
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

    // Create the Instance, Adapter, and Device. We can specify preferred backend, device name, or rendering mode. In this case we let rend3 choose for us.
    let iad = pollster::block_on(rend3::create_iad(None, None, None)).unwrap();

    // The one line of unsafe needed. We just need to guarentee that the window outlives the use of the surface.
    let surface = Arc::new(unsafe { iad.instance.create_surface(&window) });
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
    let render_texture_options = rend3_pbr::RenderTextureOptions {
        resolution: glam::UVec2::new(window_size.width, window_size.height),
        samples: rend3_pbr::SampleCount::One,
    };
    let mut pbr_routine = rend3_pbr::PbrRenderRoutine::new(&renderer, render_texture_options);
    let mut tonemapping_routine =
        rend3_pbr::TonemappingRoutine::new(&renderer, render_texture_options.resolution, format);

    // Create mesh and calculate smooth normals based on vertices.
    //
    // We do not need to keep these handles alive once we make the object
    let (mesh, material) = load_gltf(&renderer, concat!(env!("CARGO_MANIFEST_DIR"), "/data.glb"));

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::types::Object {
        mesh,
        material,
        transform: glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, -1.0)),
    };
    // We need to keep the object alive.
    let _object_handle = renderer.add_object(object);

    let view_location = glam::Vec3::new(3.0, 3.0, -5.0);
    let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, -0.49, 0.43, 0.0);
    let view = view * glam::Mat4::from_translation(-view_location);

    // Set camera's location
    renderer.set_camera_data(rend3::types::Camera {
        projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
        view,
    });

    // Create a single directional light
    //
    // We need to keep the handle alive.
    let _directional_handle = renderer.add_directional_light(rend3::types::DirectionalLight {
        color: glam::Vec3::ONE,
        intensity: 10.0,
        // Direction will be normalized
        direction: glam::Vec3::new(-1.0, -4.0, 2.0),
        distance: 400.0,
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
            pbr_routine.resize(
                &renderer,
                rend3_pbr::RenderTextureOptions {
                    resolution: size,
                    samples: rend3_pbr::SampleCount::One,
                },
            );
            tonemapping_routine.resize(size);
        }
        // Render!
        winit::event::Event::MainEventsCleared => {
            // Get a frame
            let frame = rend3::util::output::OutputFrame::Surface {
                surface: Arc::clone(&surface),
            };
            // Ready up the renderer
            let (cmd_bufs, ready) = renderer.ready();

            // Build a rendergraph
            let mut graph = rend3::RenderGraph::new();
            // Upload culling information to the GPU and into the graph.
            pbr_routine.add_pre_cull_to_graph(&mut graph);

            // Run all culling for shadows and the camera.
            pbr_routine.add_shadow_culling_to_graph(&mut graph, &ready);
            pbr_routine.add_culling_to_graph(&mut graph);

            // Render shadows
            pbr_routine.add_shadow_rendering_to_graph(&mut graph, &ready);

            // Depth prepass and forward pass.
            pbr_routine.add_prepass_to_graph(&mut graph);
            pbr_routine.add_forward_to_graph(&mut graph);

            // Tonemap onto the output.
            tonemapping_routine.add_to_graph(&mut graph);

            // Dispatch a render using the built up rendergraph!
            graph.execute(&renderer, frame, cmd_bufs, &ready);
        }
        // Other events we don't care about
        _ => {}
    });
}
