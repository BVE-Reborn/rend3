use std::time::Instant;

use egui_winit_platform::{Platform, PlatformDescriptor};
use glam::UVec2;
use winit::{event::Event::*, event_loop::ControlFlow};

fn main() {
    // Setup logging
    env_logger::init();

    // Create event loop and window
    let event_loop = winit::event_loop::EventLoop::new();
    let window = {
        let mut builder = winit::window::WindowBuilder::new();
        builder = builder.with_title("rend3 egui demo");
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
        UVec2::new(window_size.width, window_size.height),
        rend3::types::PresentMode::Mailbox,
    );

    // Make us a renderer.
    let renderer = rend3::Renderer::new(iad, Some(window_size.width as f32 / window_size.height as f32)).unwrap();

    // Create the egui render routine
    let mut routine = rend3_egui::EguiRenderRoutine::new(
        &renderer,
        format,
        1,
        window_size.width,
        window_size.height,
        window.scale_factor() as f32,
    );

    // Create the pbr pipeline with the same internal resolution and 4x multisampling
    let mut pbr_routine = rend3_pbr::PbrRenderRoutine::new(
        &renderer,
        rend3_pbr::RenderTextureOptions {
            resolution: UVec2::new(window_size.width, window_size.height),
            samples: rend3_pbr::SampleCount::Four,
        },
        format,
    );

    // Create mesh and calculate smooth normals based on vertices
    let mesh = create_mesh();

    // Add mesh to renderer's world.
    //
    // All handles are refcounted, so we only need to hang onto the handle until we make an object.
    let mesh_handle = renderer.add_mesh(mesh);

    // Add PBR material with all defaults except a single color.
    let material = rend3_pbr::material::PbrMaterial {
        albedo: rend3_pbr::material::AlbedoComponent::Value(glam::Vec4::new(0.0, 0.5, 0.5, 1.0)),
        ..rend3_pbr::material::PbrMaterial::default()
    };
    let material_handle = renderer.add_material(material);

    // Combine the mesh and the material with a location to give an object.
    let object = rend3::types::Object {
        mesh: mesh_handle,
        material: material_handle.clone(),
        transform: glam::Mat4::IDENTITY,
    };

    // Creating an object will hold onto both the mesh and the material
    // even if they are deleted.
    //
    // We need to keep the object handle alive.
    let _object_handle = renderer.add_object(object);

    let camera_pitch = std::f32::consts::FRAC_PI_4;
    let camera_yaw = -std::f32::consts::FRAC_PI_4;
    // These values may seem arbitrary, but they center the camera on the cube in the scene
    let camera_location = glam::Vec3A::new(10.0, 15.0, -10.0);
    let view  = glam::Mat4::from_euler(glam::EulerRot::XYZ, -camera_pitch, -camera_yaw, 0.0);
    let view = view * glam::Mat4::from_translation((-camera_location).into());

    // Set camera location data
    renderer.set_camera_data(rend3::types::Camera {
        projection: rend3::types::CameraProjection::Projection {
            vfov: 60.0,
            near: 0.1,
        },
        view
    });

    // Create a single directional light
    //
    // We need to keep the directional light handle alive.
    let _directional_handle = renderer.add_directional_light(rend3::types::DirectionalLight {
        color: glam::Vec3::ONE,
        intensity: 10.0,
        // Direction will be normalized
        direction: glam::Vec3::new(-1.0, -4.0, 2.0),
        distance: 400.0,
    });

    // We use the egui_winit_platform crate as the platform.
    let mut platform = Platform::new(PlatformDescriptor {
        physical_width: window_size.width as u32,
        physical_height: window_size.height as u32,
        scale_factor: window.scale_factor(),
        font_definitions: egui::FontDefinitions::default(),
        style: Default::default(),
    });

    let start_time = Instant::now();
    let mut color: [f32; 4] = [0.0, 0.5, 0.5, 1.0];

    event_loop.run(move |event, _, control_flow| {
        // Pass the winit events to the platform integration.
        platform.handle_event(&event);

        match event {
            RedrawRequested(..) => {
                platform.update_time(start_time.elapsed().as_secs_f64());
                platform.begin_frame();

                // Insert egui commands here
                let ctx = platform.context();
                egui::Window::new("Change color").resizable(true).show(&ctx, |ui| {
                    ui.label("Change the color of the cube");
                    if ui.color_edit_button_rgba_unmultiplied(&mut color).changed() {
                        renderer.update_material(
                            &material_handle.clone(),
                            rend3_pbr::material::PbrMaterial {
                                albedo: rend3_pbr::material::AlbedoComponent::Value(glam::Vec4::new(
                                    color[0], color[1], color[2], color[3],
                                )),
                                ..rend3_pbr::material::PbrMaterial::default()
                            },
                        );
                    }
                });

                // End the UI frame. Now let's draw the UI with our Backend, we could also handle the output here
                let (_output, paint_commands) = platform.end_frame(Some(&window));
                let paint_jobs = platform.context().tessellate(paint_commands);

                let input = rend3_egui::Input {
                    clipped_meshes: &paint_jobs,
                    context: platform.context(),
                };

                // Render our frame
                let frame = rend3::util::output::OutputFrame::from_surface(&surface).unwrap();

                // For now we'll have to render our routines separately
                // Check out https://github.com/BVE-Reborn/rend3/issues/229 for progress
                let _stats = renderer.render(&mut pbr_routine, (), frame.as_view());
                let _stats = renderer.render(&mut routine, &input, frame.as_view());

                frame.present();

                *control_flow = ControlFlow::Poll;
            }
            MainEventsCleared => {
                window.request_redraw();
            }
            WindowEvent { event, .. } => match event {
                winit::event::WindowEvent::Resized(size) => {
                    let size = UVec2::new(size.width, size.height);
                    // Reconfigure the surface for the new size.
                    rend3::configure_surface(
                        &surface,
                        &renderer.device,
                        format,
                        UVec2::new(size.x, size.y),
                        rend3::types::PresentMode::Mailbox,
                    );

                    routine.resize(size.x, size.y, window.scale_factor() as f32);

                    pbr_routine.resize(
                        &renderer,
                        rend3_pbr::RenderTextureOptions {
                            resolution: size,
                            samples: rend3_pbr::SampleCount::One,
                        },
                    );
                }
                winit::event::WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                }
                _ => {}
            },
            _ => {}
        }
    });
}

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
