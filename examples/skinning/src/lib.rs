use std::{path::Path, sync::Arc, time::Instant};

use rend3_gltf::GltfSceneInstance;

const SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::One;

#[derive(Default)]
struct SkinningExample {
    loaded_scene: Option<rend3_gltf::LoadedGltfScene>,
    loaded_instance: Option<rend3_gltf::GltfSceneInstance>,
    directional_light_handle: Option<rend3::types::DirectionalLightHandle>,
    armature: Option<rend3_gltf::Armature>,
    start_time: Option<Instant>,
}

/// Locates an object in the node list that corresponds to an animated mesh
/// and returns its list of skeletons. Note that a gltf object may contain
/// multiple primitives, and there will be one skeleton per primitive.
pub fn find_armature(instance: &GltfSceneInstance) -> Option<rend3_gltf::Armature> {
    for node in &instance.nodes {
        if let Some(ref obj) = node.inner.object {
            if let Some(ref armature) = obj.inner.armature {
                return Some(armature.clone());
            }
        }
    }
    None
}

impl SkinningExample {
    /// This function gets called every frame. Updates the skeleton's joint
    /// positions
    pub fn update_skeleton(&mut self, renderer: &rend3::Renderer) {
        let armature = &self.armature.as_ref().expect("Data must be loaded by now");
        let loaded_scene = &self.loaded_scene.as_ref().expect("Data must be loaded by now");
        let inverse_bind_matrices = &loaded_scene.skins[armature.skin_index].inner.inverse_bind_matrices;

        // Compute a very simple animation for the top bone
        let elapsed_time = Instant::now().duration_since(self.start_time.unwrap());
        let t = elapsed_time.as_secs_f32();
        let rotation_degrees = 30.0 * f32::sin(5.0 * t);

        // An armature contains multiple skeletons, one per mesh primitive being
        // deformed. We need to set the joint matrices per each skeleton.
        for skeleton in &armature.skeletons {
            renderer.set_skeleton_joint_transforms(
                skeleton,
                &[
                    glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, -4.18)),
                    glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, 0.0))
                        * glam::Mat4::from_rotation_x(rotation_degrees.to_radians()),
                ],
                inverse_bind_matrices,
            );
        }
    }
}

impl rend3_framework::App for SkinningExample {
    const HANDEDNESS: rend3::types::Handedness = rend3::types::Handedness::Left;

    fn sample_count(&self) -> rend3::types::SampleCount {
        SAMPLE_COUNT
    }

    fn setup(
        &mut self,
        _window: &winit::window::Window,
        renderer: &Arc<rend3::Renderer>,
        _routines: &Arc<rend3_framework::DefaultRoutines>,
        _surface_format: rend3::types::TextureFormat,
    ) {
        // Store the startup time. Use later to animate the joint rotation
        self.start_time = Some(Instant::now());

        let view_location = glam::Vec3::new(0.0, 0.0, -10.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0);
        let view = view * glam::Mat4::from_translation(-view_location);

        // Set camera's location
        renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
            view,
        });

        // Load a gltf model with animation data
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/RiggedSimple.glb"));
        let gltf_data = std::fs::read(&path).unwrap();
        let parent_directory = path.parent().unwrap();
        let (loaded_scene, loaded_instance) = pollster::block_on(rend3_gltf::load_gltf(
            renderer,
            &gltf_data,
            &rend3_gltf::GltfLoadSettings::default(),
            |p| rend3_gltf::filesystem_io_func(&parent_directory, p),
        ))
        .expect("Loading gltf scene");

        // The returned loaded model contains a node hierarchy with a complete
        // scene. We know in our case there will be a single node in the tree
        // with an armature.
        self.armature = Some(find_armature(&loaded_instance).unwrap());

        // Store the loaded model somewhere, otherwise all the data gets freed.
        self.loaded_scene = Some(loaded_scene);
        self.loaded_instance = Some(loaded_instance);

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
    }

    fn handle_event(
        &mut self,
        window: &winit::window::Window,
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
            rend3_framework::Event::WindowEvent {
                event: winit::event::WindowEvent::CloseRequested,
                ..
            } => {
                control_flow(winit::event_loop::ControlFlow::Exit);
            }
            rend3_framework::Event::MainEventsCleared => {
                self.update_skeleton(renderer);
                window.request_redraw();
            }
            // Render!
            rend3_framework::Event::RedrawRequested(_) => {
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

                // Add the default rendergraph without a skybox
                base_rendergraph.add_to_graph(
                    &mut graph,
                    &ready,
                    &pbr_routine,
                    None,
                    &tonemapping_routine,
                    resolution,
                    SAMPLE_COUNT,
                    glam::Vec4::splat(0.15),
                );

                // Dispatch a render using the built up rendergraph!
                graph.execute(renderer, frame, cmd_bufs, &ready);
            }
            // Other events we don't care about
            _ => {}
        }
    }
}

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on", logger(level = "debug")))]
pub fn main() {
    let app = SkinningExample::default();
    rend3_framework::start(
        app,
        winit::window::WindowBuilder::new()
            .with_title("skinning-example")
            .with_maximized(true),
    );
}
