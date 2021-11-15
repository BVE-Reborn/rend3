use std::sync::Arc;

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

#[derive(Default)]
struct CubeExample {
    object_handle: Option<rend3::types::ObjectHandle>,
    directional_light_handle: Option<rend3::types::DirectionalLightHandle>,
}

impl rend3_framework::App for CubeExample {
    fn setup<'a>(
        &'a mut self,
        _window: &'a winit::window::Window,
        renderer: &'a rend3::Renderer,
        _routines: &'a rend3_framework::DefaultRoutines,
        _surface: &'a rend3::types::Surface,
    ) -> std::pin::Pin<Box<dyn rend3_framework::NativeSendFuture<()> + 'a>> {
        Box::pin(async move {
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
                material: material_handle,
                transform: glam::Mat4::IDENTITY,
            };
            // Creating an object will hold onto both the mesh and the material
            // even if they are deleted.
            //
            // We need to keep the object handle alive.
            self.object_handle = Some(renderer.add_object(object));

            let view_location = glam::Vec3::new(3.0, 3.0, -5.0);
            let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, -0.55, 0.5, 0.0);
            let view = view * glam::Mat4::from_translation(-view_location);

            // Set camera's location
            renderer.set_camera_data(rend3::types::Camera {
                projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
                view,
            });

            // Create a single directional light
            //
            // We need to keep the directional light handle alive.
            self.directional_light_handle = Some(renderer.add_directional_light(rend3::types::DirectionalLight {
                color: glam::Vec3::ONE,
                intensity: 10.0,
                // Direction will be normalized
                direction: glam::Vec3::new(-1.0, -4.0, 2.0),
                distance: 400.0,
            }));
        })
    }

    fn handle_event<'a, T: rend3_framework::NativeSend>(
        &mut self,
        _window: &'a winit::window::Window,
        renderer: &'a Arc<rend3::Renderer>,
        routines: &'a Arc<rend3_framework::DefaultRoutines>,
        surface: &'a Arc<rend3::types::Surface>,
        event: rend3_framework::Event,
        control_flow: impl FnOnce(winit::event_loop::ControlFlow) + rend3_framework::NativeSend + 'a,
    ) -> std::pin::Pin<Box<dyn rend3_framework::NativeSendFuture<()> + 'a>> {
        Box::pin(async move {
            match event {
                // Close button was clicked, we should close.
                rend3_framework::Event::WindowEvent {
                    event: winit::event::WindowEvent::CloseRequested,
                    ..
                } => {
                    control_flow(winit::event_loop::ControlFlow::Exit);
                }
                // Render!
                rend3_framework::Event::MainEventsCleared => {
                    // Get a frame
                    let frame = rend3::util::output::OutputFrame::Surface {
                        surface: Arc::clone(surface),
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

                    // Dispatch a render using the built up rendergraph!
                    graph.execute(renderer, frame, cmd_bufs, &ready);
                }
                // Other events we don't care about
                _ => {}
            }
        })
    }
}

fn main() {
    let app = CubeExample::default();
    rend3_framework::start(app, winit::window::WindowBuilder::new().with_title("cube-example"));
}
