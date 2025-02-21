use image::GenericImageView;

fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn uv(pos: [f32; 2]) -> glam::Vec2 {
    glam::Vec2::from(pos)
}

fn create_quad(size: f32) -> rend3::types::Mesh {
    let vertex_positions = [
        vertex([-size * 0.5, size * 0.5, 0.0]),
        vertex([size * 0.5, size * 0.5, 0.0]),
        vertex([size * 0.5, -size * 0.5, 0.0]),
        vertex([-size * 0.5, -size * 0.5, 0.0]),
    ];
    let uv_positions = [uv([0.0, 0.0]), uv([1.0, 0.0]), uv([1.0, 1.0]), uv([0.0, 1.0])];
    let index_data: &[u32] = &[0, 1, 2, 2, 3, 0];

    rend3::types::MeshBuilder::new(vertex_positions.to_vec(), rend3::types::Handedness::Left)
        .with_vertex_texture_coordinates_0(uv_positions.to_vec())
        .with_indices(index_data.to_vec())
        .build()
        .unwrap()
}

const CAMERA_DEPTH: f32 = 10.0;

struct TexturedQuadExampleData {
    _object_handle: rend3::types::ObjectHandle,
    view: glam::Mat4,
}

const SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::One;

#[derive(Default)]
struct TexturedQuadExample {
    data: Option<TexturedQuadExampleData>,
}
impl rend3_framework::App for TexturedQuadExample {
    /// Set handedness of coordinate system
    fn get_handedness(&self) -> rend3::types::Handedness {
        rend3::types::Handedness::Left    // default
    }


    fn sample_count(&self) -> rend3::types::SampleCount {
        SAMPLE_COUNT
    }

    fn setup(&mut self, context: rend3_framework::SetupContext<'_>) {
        // Create mesh and calculate smooth normals based on vertices
        let mesh = create_quad(300.0);

        // Add mesh to renderer's world.
        //
        // All handles are refcounted, so we only need to hang onto the handle until we
        // make an object.
        let mesh_handle = context.renderer.add_mesh(mesh).unwrap();

        // Add texture to renderer's world.
        let image_checker =
            image::load_from_memory(include_bytes!("checker.png")).expect("Failed to load image from memory");
        let image_checker_rgba8 = image_checker.to_rgba8();
        let texture_checker = rend3::types::Texture {
            label: Option::None,
            data: image_checker_rgba8.to_vec(),
            format: rend3::types::TextureFormat::Rgba8UnormSrgb,
            size: glam::UVec2::new(image_checker.dimensions().0, image_checker.dimensions().1),
            mip_count: rend3::types::MipmapCount::ONE,
            mip_source: rend3::types::MipmapSource::Uploaded,
        };
        let texture_checker_handle = context.renderer.add_texture_2d(texture_checker).unwrap();

        // Add PBR material with all defaults except a single color.
        let material = rend3_routine::pbr::PbrMaterial {
            albedo: rend3_routine::pbr::AlbedoComponent::Texture(texture_checker_handle),
            unlit: true,
            sample_type: rend3_routine::pbr::SampleType::Nearest,
            ..rend3_routine::pbr::PbrMaterial::default()
        };
        let material_handle = context.renderer.add_material(material);

        // Combine the mesh and the material with a location to give an object.
        let object = rend3::types::Object {
            mesh_kind: rend3::types::ObjectMeshKind::Static(mesh_handle),
            material: material_handle,
            transform: glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::new(1.0, 1.0, 1.0),
                glam::Quat::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0),
                glam::Vec3::new(0.0, 0.0, 0.0),
            ),
        };

        // Creating an object will hold onto both the mesh and the material
        // even if they are deleted.
        //
        // We need to keep the object handle alive.
        let _object_handle = context.renderer.add_object(object);

        let view_location = glam::Vec3::new(0.0, 0.0, -1.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0);
        let view = view * glam::Mat4::from_translation(-view_location);

        // Set camera's location
        context.renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Orthographic {
                size: glam::Vec3A::new(context.resolution.x as f32, context.resolution.y as f32, CAMERA_DEPTH),
            },
            view,
        });

        self.data = Some(TexturedQuadExampleData { _object_handle, view })
    }

    fn handle_event(&mut self, context: rend3_framework::EventContext<'_>, event: winit::event::Event<()>) {
        if let winit::event::Event::WindowEvent { event: winit::event::WindowEvent::Resized(size), .. } = event {
            let size = glam::UVec2::new(size.width, size.height);
            // Reset camera
            context.renderer.set_camera_data(rend3::types::Camera {
                projection: rend3::types::CameraProjection::Orthographic {
                    size: glam::Vec3A::new(size.x as f32, size.y as f32, CAMERA_DEPTH),
                },
                view: self.data.as_ref().unwrap().view,
            });
        }
    }

    fn handle_redraw(&mut self, context: rend3_framework::RedrawContext<'_>) {
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
        // Add the default rendergraph
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

        // Dispatch a render using the built up rendergraph!
        graph.execute(context.renderer, &mut eval_output);
    }
}

pub fn main() {
    let app = TexturedQuadExample::default();
    rend3_framework::start(app, winit::window::WindowAttributes::default().with_title("textured-quad").with_maximized(true))
}

#[cfg(test)]
#[rend3_test::test_attr]
async fn test() {
    crate::tests::test_app(crate::tests::TestConfiguration {
        app: TexturedQuadExample::default(),
        reference_path: "src/textured_quad/screenshot.png",
        size: glam::UVec2::new(1280, 720),
        threshold_set: rend3_test::Threshold::Mean(0.0).into(),
    })
    .await
    .unwrap();
}
