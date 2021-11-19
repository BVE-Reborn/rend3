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
        .build()
        .unwrap();

    // Add mesh to renderer's world
    let mesh_handle = renderer.add_mesh(mesh);

    // Add basic material with all defaults except a single color.
    let material = primitive.material();
    let metallic_roughness = material.pbr_metallic_roughness();
    let material_handle = renderer.add_material(rend3_routine::material::PbrMaterial {
        albedo: rend3_routine::material::AlbedoComponent::Value(metallic_roughness.base_color_factor().into()),
        ..Default::default()
    });

    (mesh_handle, material_handle)
}

#[derive(Default)]
struct GltfExample {
    object_handle: Option<rend3::types::ObjectHandle>,
    directional_light_handle: Option<rend3::types::DirectionalLightHandle>,
}

impl rend3_framework::App for GltfExample {
    fn setup<'a>(
        &'a mut self,
        _window: &'a winit::window::Window,
        renderer: &'a rend3::Renderer,
        _routines: &'a rend3_framework::DefaultRoutines,
        _surface: &'a rend3::types::Surface,
        _surface_format: rend3::types::TextureFormat,
    ) -> std::pin::Pin<Box<dyn rend3_framework::NativeSendFuture<()> + 'a>> {
        Box::pin(async move {
            // Create mesh and calculate smooth normals based on vertices.
            //
            // We do not need to keep these handles alive once we make the object
            let (mesh, material) = load_gltf(renderer, concat!(env!("CARGO_MANIFEST_DIR"), "/data.glb"));

            // Combine the mesh and the material with a location to give an object.
            let object = rend3::types::Object {
                mesh,
                material,
                transform: glam::Mat4::from_scale(glam::Vec3::new(1.0, 1.0, -1.0)),
            };
            // We need to keep the object alive.
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

    fn handle_event<'a>(
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
    let app = GltfExample::default();
    rend3_framework::start(
        app,
        winit::window::WindowBuilder::new()
            .with_title("gltf-example")
            .with_maximized(true),
    );
}
