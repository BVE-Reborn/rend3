use std::{path::Path, sync::Arc};

const SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::One;

struct AnimationExample {
    loaded_scene: Option<rend3_gltf::LoadedGltfScene>,
    loaded_instance: Option<rend3_gltf::GltfSceneInstance>,
    animation_data: Option<rend3_anim::AnimationData>,
    directional_light_handle: Option<rend3::types::DirectionalLightHandle>,
    animation_time: f32,
    last_frame_time: instant::Instant,
}

impl AnimationExample {
    pub fn update(&mut self, renderer: &rend3::Renderer, delta: f32) {
        let scene = self.loaded_scene.as_ref().unwrap();
        self.animation_time = (self.animation_time + delta) % scene.animations[0].inner.duration;
        rend3_anim::pose_animation_frame(
            renderer,
            self.loaded_scene.as_ref().unwrap(),
            self.loaded_instance.as_ref().unwrap(),
            self.animation_data.as_ref().unwrap(),
            0,
            self.animation_time,
        )
    }
}

impl rend3_framework::App for AnimationExample {
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
        let view_location = glam::Vec3::new(0.0, 1.5, -5.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0);
        let view = view * glam::Mat4::from_translation(-view_location);

        // Set camera's location
        renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
            view,
        });

        // Load a gltf model with animation data
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/scene.gltf"));
        let gltf_data = std::fs::read(&path).unwrap();
        let parent_directory = path.parent().unwrap();
        let (loaded_scene, loaded_instance) = pollster::block_on(rend3_gltf::load_gltf(
            renderer,
            &gltf_data,
            &rend3_gltf::GltfLoadSettings::default(),
            |p| rend3_gltf::filesystem_io_func(&parent_directory, p),
        ))
        .expect("Loading gltf scene");

        // Store the loaded model somewhere, otherwise all the data gets freed.
        self.animation_data = Some(rend3_anim::AnimationData::from_gltf_scene(
            &loaded_scene,
            &loaded_instance,
        ));
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
                let now = instant::Instant::now();
                let delta = now.duration_since(self.last_frame_time).as_secs_f32();
                self.last_frame_time = now;
                self.update(renderer, delta);
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

// Instant is not `Default` so we need a manual impl for Default
impl Default for AnimationExample {
    fn default() -> Self {
        Self {
            loaded_scene: Default::default(),
            loaded_instance: Default::default(),
            animation_data: Default::default(),
            directional_light_handle: Default::default(),
            animation_time: Default::default(),
            last_frame_time: instant::Instant::now(),
        }
    }
}

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on", logger(level = "debug")))]
pub fn main() {
    let app = AnimationExample::default();
    rend3_framework::start(
        app,
        winit::window::WindowBuilder::new()
            .with_title("skinning-example")
            .with_maximized(true),
    );
}

