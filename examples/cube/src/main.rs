use glam::UVec2;
use rend3::RenderRoutine;

fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn create_mesh() -> rend3::types::Mesh {
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

    rend3::types::MeshBuilder::new(vertex_positions.to_vec())
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
        builder = builder.with_title("rend3 cube");
        builder.build(&event_loop).expect("Could not build window")
    };

    let window_size = window.inner_size();

    let renderer = pollster::block_on(
        rend3::RendererBuilder::new(rend3::InternalSurfaceOptions {
            vsync: rend3::VSyncMode::On,
            size: UVec2::new(window_size.width, window_size.height),
        })
        .window(&window)
        .build(),
    )
    .unwrap();

    // Create the pbr pipeline with the same internal resolution and 4x multisampling
    let mut routine = rend3_pbr::PbrRenderRoutine::new(
        &renderer,
        rend3_pbr::RenderTextureOptions {
            resolution: UVec2::new(window_size.width, window_size.height),
            samples: rend3_pbr::SampleCount::Four,
        },
    );

    // Create mesh and calculate smooth normals based on vertices
    let mesh = create_mesh();

    // Add mesh to renderer's world.
    //
    // All handles are refcounted, so we only need to hang onto the handle until we make an object.
    let mesh_handle = renderer.add_mesh(mesh);

    // Add basic material with all defaults except a single color.
    let material = rend3::types::Material {
        albedo: rend3::types::AlbedoComponent::Value(glam::Vec4::new(0.0, 0.5, 0.5, 1.0)),
        ..rend3::types::Material::default()
    };
    let material_handle = renderer.add_material(material);

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::types::Object {
        mesh: mesh_handle,
        material: material_handle,
        transform: glam::Mat4::IDENTITY,
    };
    // Creating an object will hold onto both the mesh and the material
    // even if they are deleted.
    //
    // We need to keep the object handle alive.
    let _object_handle = renderer.add_object(object);

    // Set camera's location
    renderer.set_camera_data(rend3::types::Camera {
        projection: rend3::types::CameraProjection::Projection {
            vfov: 60.0,
            near: 0.1,
            pitch: 0.5,
            yaw: -0.55,
        },
        location: glam::Vec3A::new(3.0, 3.0, -5.0),
    });

    // Create a single directional light
    //
    // We need to keep the directional light handle alive.
    let _directional_handle = renderer.add_directional_light(rend3::types::DirectionalLight {
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
            let size = UVec2::new(size.width, size.height);
            renderer.set_internal_surface_options(rend3::InternalSurfaceOptions {
                vsync: rend3::VSyncMode::Off,
                size,
            });
            routine.resize(
                &renderer,
                rend3_pbr::RenderTextureOptions {
                    resolution: size,
                    samples: rend3_pbr::SampleCount::Four,
                },
            );
        }
        // Render!
        winit::event::Event::MainEventsCleared => {
            // Dispatch a render!
            let dynref: &mut dyn RenderRoutine = &mut routine;
            let _stats = renderer.render(dynref, rend3::util::output::RendererOutput::InternalSurface);
        }
        // Other events we don't care about
        _ => {}
    });
}
