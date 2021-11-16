use std::sync::Arc;

struct EguiExampleData {
    _object_handle: rend3::types::ObjectHandle,
    material_handle: rend3::types::MaterialHandle,
    _directional_handle: rend3::types::DirectionalLightHandle,

    egui_routine: rend3_egui::EguiRenderRoutine,
    platform: egui_winit_platform::Platform,
    start_time: instant::Instant,
    color: [f32; 4],
}

#[derive(Default)]
struct EguiExample {
    data: Option<EguiExampleData>,
}
impl rend3_framework::App for EguiExample {
    fn setup<'a>(
        &'a mut self,
        window: &'a winit::window::Window,
        renderer: &'a rend3::Renderer,
        _routines: &'a rend3_framework::DefaultRoutines,
        _surface: &'a rend3::types::Surface,
        surface_format: rend3::types::TextureFormat,
    ) -> std::pin::Pin<Box<dyn rend3_framework::NativeSendFuture<()> + 'a>> {
        Box::pin(async move {
            let window_size = window.inner_size();

            // Create the egui render routine
            let egui_routine = rend3_egui::EguiRenderRoutine::new(
                &renderer,
                surface_format,
                1, // For now this has to be 1, until rendergraphs support multisampling
                window_size.width,
                window_size.height,
                window.scale_factor() as f32,
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
            let camera_location = glam::Vec3A::new(5.0, 7.5, -5.0);
            let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, -camera_pitch, -camera_yaw, 0.0);
            let view = view * glam::Mat4::from_translation((-camera_location).into());

            // Set camera location data
            renderer.set_camera_data(rend3::types::Camera {
                projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
                view,
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

            // Create the winit/egui integration, which manages our egui context for us.
            let platform = egui_winit_platform::Platform::new(egui_winit_platform::PlatformDescriptor {
                physical_width: window_size.width as u32,
                physical_height: window_size.height as u32,
                scale_factor: window.scale_factor(),
                font_definitions: egui::FontDefinitions::default(),
                style: Default::default(),
            });

            let start_time = instant::Instant::now();
            let color: [f32; 4] = [0.0, 0.5, 0.5, 1.0];

            self.data = Some(EguiExampleData {
                _object_handle,
                material_handle,
                _directional_handle,

                egui_routine,
                platform,
                start_time,
                color,
            })
        })
    }

    fn handle_event<'a>(
        &'a mut self,
        window: &'a winit::window::Window,
        renderer: &'a Arc<rend3::Renderer>,
        routines: &'a Arc<rend3_framework::DefaultRoutines>,
        surface: &'a Arc<rend3::types::Surface>,
        event: rend3_framework::Event,
        control_flow: impl FnOnce(winit::event_loop::ControlFlow) + rend3_framework::NativeSend + 'a,
    ) -> std::pin::Pin<Box<dyn rend3_framework::NativeSendFuture<()> + 'a>> {
        Box::pin(async move {
            let data = self.data.as_mut().unwrap();

            // Pass the winit events to the platform integration.
            data.platform.handle_event(&event);

            match event {
                rend3_framework::Event::RedrawRequested(..) => {
                    data.platform.update_time(data.start_time.elapsed().as_secs_f64());
                    data.platform.begin_frame();

                    // Insert egui commands here
                    let ctx = data.platform.context();
                    egui::Window::new("Change color").resizable(true).show(&ctx, |ui| {
                        ui.label("Change the color of the cube");
                        if ui.color_edit_button_rgba_unmultiplied(&mut data.color).changed() {
                            renderer.update_material(
                                &data.material_handle.clone(),
                                rend3_pbr::material::PbrMaterial {
                                    albedo: rend3_pbr::material::AlbedoComponent::Value(glam::Vec4::from(data.color)),
                                    ..rend3_pbr::material::PbrMaterial::default()
                                },
                            );
                        }
                    });

                    // End the UI frame. Now let's draw the UI with our Backend, we could also handle the output here
                    let (_output, paint_commands) = data.platform.end_frame(Some(&window));
                    let paint_jobs = data.platform.context().tessellate(paint_commands);

                    let input = rend3_egui::Input {
                        clipped_meshes: &paint_jobs,
                        context: data.platform.context(),
                    };

                    // Get a frame
                    let frame = rend3::util::output::OutputFrame::Surface {
                        surface: Arc::clone(&surface),
                    };

                    // Ready up the renderer
                    let (cmd_bufs, ready) = renderer.ready();

                    // Lock the routines
                    let pbr_routine = routines.pbr.lock().await;
                    let tonemapping_routine = routines.tonemapping.lock().await;

                    // Build a rendergraph
                    let mut graph = rend3::RenderGraph::new();
                    // Upload culling information to the GPU and into the graph.
                    pbr_routine.add_pre_cull_to_graph(&mut graph);

                    // Run all culling for shadows and the camera.
                    pbr_routine.add_shadow_culling_to_graph(&mut graph, &ready);
                    pbr_routine.add_culling_to_graph(&mut graph);

                    // Render shadows.
                    pbr_routine.add_shadow_rendering_to_graph(&mut graph, &ready);

                    // Depth prepass and forward pass.
                    pbr_routine.add_prepass_to_graph(&mut graph);
                    pbr_routine.add_forward_to_graph(&mut graph);

                    // Tonemap onto the output.
                    tonemapping_routine.add_to_graph(&mut graph);

                    // Add egui on top of all the other passes
                    data.egui_routine.add_to_graph(&mut graph, input);

                    // Dispatch a render using the built up rendergraph!
                    graph.execute(&renderer, frame, cmd_bufs, &ready);

                    control_flow(winit::event_loop::ControlFlow::Poll);
                }
                rend3_framework::Event::MainEventsCleared => {
                    window.request_redraw();
                }
                rend3_framework::Event::WindowEvent { event, .. } => match event {
                    winit::event::WindowEvent::Resized(size) => {
                        data.egui_routine
                            .resize(size.width, size.height, window.scale_factor() as f32);
                    }
                    winit::event::WindowEvent::CloseRequested => {
                        control_flow(winit::event_loop::ControlFlow::Exit);
                    }
                    _ => {}
                },
                _ => {}
            }
        })
    }
}

fn main() {
    let app = EguiExample::default();
    rend3_framework::start(
        app,
        winit::window::WindowBuilder::new()
            .with_title("egui")
            .with_maximized(true),
    )
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
