struct EguiExampleData {
    _object_handle: rend3::types::ObjectHandle,
    material_handle: rend3::types::MaterialHandle,
    _directional_handle: rend3::types::DirectionalLightHandle,

    egui_routine: rend3_egui::EguiRenderRoutine,
    context: egui::Context,
    platform: Option<egui_winit::State>,
    color: [f32; 4],
}

const SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::One;

#[derive(Default)]
struct EguiExample {
    data: Option<EguiExampleData>,
    rust_logo: egui::TextureId,
}
impl rend3_framework::App for EguiExample {
    /// Set handedness of coordinate system
    fn get_handedness(&self) -> rend3::types::Handedness {
        rend3::types::Handedness::Left    // default
    }


    fn sample_count(&self) -> rend3::types::SampleCount {
        SAMPLE_COUNT
    }

    fn setup(&mut self, context: rend3_framework::SetupContext<'_>) {
        let window_size = context.resolution;

        // Create the egui render routine
        let mut egui_routine = rend3_egui::EguiRenderRoutine::new(
            context.renderer,
            context.surface_format,
            rend3::types::SampleCount::One,
            window_size.x,
            window_size.y,
            context.scale_factor,
        );

        // Create mesh and calculate smooth normals based on vertices
        let mesh = create_mesh();

        // Add mesh to renderer's world.
        //
        // All handles are refcounted, so we only need to hang onto the handle until we
        // make an object.
        let mesh_handle = context.renderer.add_mesh(mesh).unwrap();

        // Add PBR material with all defaults except a single color.
        let material = rend3_routine::pbr::PbrMaterial {
            albedo: rend3_routine::pbr::AlbedoComponent::Value(glam::Vec4::new(0.0, 0.5, 0.5, 1.0)),
            transparency: rend3_routine::pbr::Transparency::Blend,
            ..rend3_routine::pbr::PbrMaterial::default()
        };
        let material_handle = context.renderer.add_material(material);

        // Combine the mesh and the material with a location to give an object.
        let object = rend3::types::Object {
            mesh_kind: rend3::types::ObjectMeshKind::Static(mesh_handle),
            material: material_handle.clone(),
            transform: glam::Mat4::IDENTITY,
        };

        // Creating an object will hold onto both the mesh and the material
        // even if they are deleted.
        //
        // We need to keep the object handle alive.
        let _object_handle = context.renderer.add_object(object);

        let camera_pitch = std::f32::consts::FRAC_PI_4;
        let camera_yaw = -std::f32::consts::FRAC_PI_4;
        // These values may seem arbitrary, but they center the camera on the cube in
        // the scene
        let camera_location = glam::Vec3A::new(5.0, 7.5, -5.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, -camera_pitch, -camera_yaw, 0.0);
        let view = view * glam::Mat4::from_translation((-camera_location).into());

        // Set camera location data
        context.renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
            view,
        });

        // Create a single directional light
        //
        // We need to keep the directional light handle alive.
        let _directional_handle = context.renderer.add_directional_light(rend3::types::DirectionalLight {
            color: glam::Vec3::ONE,
            intensity: 10.0,
            // Direction will be normalized
            direction: glam::Vec3::new(-1.0, -4.0, 2.0),
            distance: 400.0,
            resolution: 2048,
        });

        // Create the egui context
        let egui_context = egui::Context::default();
        // Create the winit/egui integration.
        let platform = if let Some(windowing) = context.windowing {
            Some(egui_winit::State::new(
                egui_context.clone(),
                egui::ViewportId::default(),
                &windowing.window,
                Some(context.scale_factor),
                None,
                None, // (JN) New max texture size field, not used.
            ))
        } else {
            None
        };

        //Images
        let image_bytes = include_bytes!("images/rust-logo-128x128-blk.png");
        let image_image = image::load_from_memory(image_bytes).unwrap();
        let image_rgba = image_image.as_rgba8().unwrap().clone().into_raw();

        use image::GenericImageView;
        let dimensions = image_image.dimensions();

        let format = wgpu::TextureFormat::Rgba8UnormSrgb;

        self.rust_logo = rend3_egui::EguiRenderRoutine::create_egui_texture(
            &mut egui_routine.internal,
            context.renderer,
            format,
            &image_rgba,
            dimensions,
            Some("rust_logo_texture"),
        );

        let color: [f32; 4] = [0.0, 0.5, 0.5, 1.0];

        self.data = Some(EguiExampleData {
            _object_handle,
            material_handle,
            _directional_handle,

            egui_routine,
            context: egui_context,
            platform,
            color,
        });
    }

    fn handle_event(&mut self, context: rend3_framework::EventContext<'_>, event: winit::event::Event<()>) {
        let data = self.data.as_mut().unwrap();

        #[allow(clippy::single_match)]
        match event {
            winit::event::Event::WindowEvent { event, .. } => {
                if let Some(window) = context.window {
                    // Pass the window events to the egui integration.
                    if data.platform.as_mut().unwrap().on_window_event(window, &event).consumed {
                        return;
                    }

                    #[allow(clippy::single_match)]
                    match event {
                        winit::event::WindowEvent::Resized(size) => {
                            data.egui_routine.resize(size.width, size.height, window.scale_factor() as f32);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_redraw(&mut self, context: rend3_framework::RedrawContext<'_>) {
        let data = self.data.as_mut().unwrap();

        let input = if let Some(window) = context.window {
            data.platform.as_mut().unwrap().take_egui_input(window)
        } else {
            egui::RawInput::default()
        };

        data.context.begin_pass(input);

        // Insert egui commands here
        let ctx = &data.context;
        egui::Window::new("Change color").resizable(true).show(ctx, |ui| {
            ui.label("Change the color of the cube");
            if ui.color_edit_button_rgba_unmultiplied(&mut data.color).changed() {
                context.renderer.update_material(
                    &data.material_handle.clone(),
                    rend3_routine::pbr::PbrMaterial {
                        albedo: rend3_routine::pbr::AlbedoComponent::Value(glam::Vec4::from(data.color)),
                        transparency: rend3_routine::pbr::Transparency::Blend,
                        ..rend3_routine::pbr::PbrMaterial::default()
                    },
                );
            }
            ui.label("Want to get rusty?");
            if ui.add(egui::widgets::ImageButton::new((self.rust_logo, egui::Vec2::splat(64.0)))).clicked() {
                webbrowser::open("https://www.rust-lang.org").expect("failed to open URL");
            }
        });

        let scale_factor = context.window.map(|w| w.scale_factor() as f32).unwrap_or(1.0);

        // End the UI frame. Now let's draw the UI with our Backend, we could also
        // handle the output here
        let egui::FullOutput { shapes, textures_delta, .. } = data.context.end_pass();
        let paint_jobs = data.context.tessellate(shapes, scale_factor);

        let input = rend3_egui::Input { clipped_meshes: &paint_jobs, textures_delta, context: data.context.clone() };

        // Swap the instruction buffers so that our frame's changes can be processed.
        context.renderer.swap_instruction_buffers();
        // Evaluate our frame's world-change instructions
        let mut eval_output = context.renderer.evaluate_instructions();

        // Lock the routines
        let pbr_routine = rend3_framework::lock(&context.routines.pbr);
        let tonemapping_routine = rend3_framework::lock(&context.routines.tonemapping);

        // Build a rendergraph
        let mut graph = rend3::graph::RenderGraph::new();

        // Import the surface texture into the render graph.
        let frame_handle = graph.add_imported_render_target(
            context.surface_texture,
            0..1,
            0..1,
            rend3::graph::ViewportRect::from_size(context.resolution),
        );
        // Add the default rendergraph without a skybox
        context.base_rendergraph.add_to_graph(
            &mut graph,
            rend3_routine::base::BaseRenderGraphInputs {
                eval_output: &eval_output,
                routines: rend3_routine::base::BaseRenderGraphRoutines {
                    pbr: &pbr_routine,
                    skybox: None,
                    tonemapping: &tonemapping_routine,
                },
                target: rend3_routine::base::OutputRenderTarget {
                    handle: frame_handle,
                    resolution: context.resolution,
                    samples: SAMPLE_COUNT,
                },
            },
            rend3_routine::base::BaseRenderGraphSettings {
                ambient_color: glam::Vec4::ZERO,
                clear_color: glam::Vec4::new(0.10, 0.05, 0.10, 1.0), // Nice scene-referred purple
            },
        );

        // Add egui on top of all the other passes
        data.egui_routine.add_to_graph(&mut graph, input, frame_handle);

        // Dispatch a render using the built up rendergraph!
        graph.execute(context.renderer, &mut eval_output);
    }
}

pub fn main() {
    let app = EguiExample::default();
    rend3_framework::start(app, winit::window::WindowAttributes::default().with_title("egui").with_maximized(true))
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

    rend3::types::MeshBuilder::new(vertex_positions.to_vec(), rend3::types::Handedness::Left)
        .with_indices(index_data.to_vec())
        .build()
        .unwrap()
}

#[cfg(test)]
#[rend3_test::test_attr]
async fn test() {
    crate::tests::test_app(crate::tests::TestConfiguration {
        app: EguiExample::default(),
        reference_path: "src/egui/screenshot.png",
        size: glam::UVec2::new(1280, 720),
        threshold_set: rend3_test::Threshold::Mean(0.01).into(),
    })
    .await
    .unwrap();
}
