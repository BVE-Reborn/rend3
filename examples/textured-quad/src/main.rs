use std::sync::Arc;

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
        .with_vertex_uv0(uv_positions.to_vec())
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
    const HANDEDNESS: rend3::types::Handedness = rend3::types::Handedness::Left;

    fn sample_count(&self) -> rend3::types::SampleCount {
        SAMPLE_COUNT
    }

    fn setup(
        &mut self,
        window: &winit::window::Window,
        renderer: &Arc<rend3::Renderer>,
        _routines: &Arc<rend3_framework::DefaultRoutines>,
        _surface_format: rend3::types::TextureFormat,
    ) {
        // Create mesh and calculate smooth normals based on vertices
        let mesh = create_quad(300.0);

        // Add mesh to renderer's world.
        //
        // All handles are refcounted, so we only need to hang onto the handle until we
        // make an object.
        let mesh_handle = renderer.add_mesh(mesh);

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
        let texture_checker_handle = renderer.add_texture_2d(texture_checker);

        // Add PBR material with all defaults except a single color.
        let material = rend3_routine::pbr::PbrMaterial {
            albedo: rend3_routine::pbr::AlbedoComponent::Texture(texture_checker_handle),
            unlit: true,
            sample_type: rend3_routine::pbr::SampleType::Nearest,
            ..rend3_routine::pbr::PbrMaterial::default()
        };
        let material_handle = renderer.add_material(material);

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
        let _object_handle = renderer.add_object(object);

        let view_location = glam::Vec3::new(0.0, 0.0, -1.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0);
        let view = view * glam::Mat4::from_translation(-view_location);

        // Set camera's location
        renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Orthographic {
                size: glam::Vec3A::new(
                    window.inner_size().width as f32,
                    window.inner_size().height as f32,
                    CAMERA_DEPTH,
                ),
            },
            view,
        });

        self.data = Some(TexturedQuadExampleData { _object_handle, view })
    }

    fn handle_event(
        &mut self,
        _window: &winit::window::Window,
        renderer: &Arc<rend3::Renderer>,
        routines: &Arc<rend3_framework::DefaultRoutines>,
        base_rendergraph: &rend3_routine::base::BaseRenderGraph,
        surface: Option<&Arc<rend3::types::Surface>>,
        resolution: glam::UVec2,
        event: rend3_framework::Event<'_, ()>,
        control_flow: impl FnOnce(winit::event_loop::ControlFlow),
    ) {
        match event {
            // Close button was clicked, we should close.
            winit::event::Event::WindowEvent {
                event: winit::event::WindowEvent::CloseRequested,
                ..
            } => {
                control_flow(winit::event_loop::ControlFlow::Exit);
            }
            // Window was resized, need to resize renderer.
            winit::event::Event::WindowEvent {
                event: winit::event::WindowEvent::Resized(size),
                ..
            } => {
                let size = glam::UVec2::new(size.width, size.height);
                // Reset camera
                renderer.set_camera_data(rend3::types::Camera {
                    projection: rend3::types::CameraProjection::Orthographic {
                        size: glam::Vec3A::new(size.x as f32, size.y as f32, CAMERA_DEPTH),
                    },
                    view: self.data.as_ref().unwrap().view,
                });
            }
            // Render!
            winit::event::Event::MainEventsCleared => {
                // Get a frame
                let frame = rend3::util::output::OutputFrame::Surface {
                    surface: Arc::clone(surface.unwrap()),
                };

                // Ready up the renderer
                let (cmd_bufs, ready) = renderer.ready();

                // Lock the routines
                let pbr_routine = rend3_framework::lock(&routines.pbr);
                let tonemapping_routine = rend3_framework::lock(&routines.tonemapping);

                // Build a rendergraph
                let mut graph = rend3::graph::RenderGraph::new();

                // Add the default rendergraph
                base_rendergraph.add_to_graph(
                    &mut graph,
                    &ready,
                    &pbr_routine,
                    None,
                    &tonemapping_routine,
                    resolution,
                    SAMPLE_COUNT,
                    glam::Vec4::ZERO,
                );

                // Dispatch a render using the built up rendergraph!
                graph.execute(renderer, frame, cmd_bufs, &ready);
            }
            // Other events we don't care about
            _ => {}
        }
    }
}

fn main() {
    let app = TexturedQuadExample::default();
    rend3_framework::start(
        app,
        winit::window::WindowBuilder::new()
            .with_title("textured-quad")
            .with_maximized(true),
    )
}
