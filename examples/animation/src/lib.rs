use rend3::types::DirectionalLightHandle;
use std::{path::Path, sync::Arc};

const SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::Four;

/// The application data, can only be obtained at `setup` time, so it's under an
/// Option in the main struct.
struct AnimatedObject {
    loaded_scene: rend3_gltf::LoadedGltfScene,
    loaded_instance: rend3_gltf::GltfSceneInstance,
    animation_data: rend3_anim::AnimationData,
    animation_time: f32,
    last_frame_time: instant::Instant,
}

#[derive(Default)]
struct AnimationExample {
    /// The application data
    animated_objects: Vec<AnimatedObject>,
    _directional_light_handle: Option<DirectionalLightHandle>,
}

fn update(renderer: &rend3::Renderer, delta: f32, animated_object: &mut AnimatedObject) {
    animated_object.animation_time =
        (animated_object.animation_time + delta) % animated_object.loaded_scene.animations[0].inner.duration;
    rend3_anim::pose_animation_frame(
        renderer,
        &animated_object.loaded_scene,
        &animated_object.loaded_instance,
        &animated_object.animation_data,
        0,
        animated_object.animation_time,
    );
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
        let view_location = glam::Vec3::new(0.0, -1.5, 5.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0);
        let view = view * glam::Mat4::from_translation(view_location);

        // Set camera's location
        renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
            view,
        });

        // Load a gltf model with animation data
        // Needs to be stored somewhere, otherwise all the data gets freed.
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/scene.gltf"));
        let gltf_data = std::fs::read(&path).unwrap();
        let parent_directory = path.parent().unwrap();
        let (loaded_scene, loaded_instance) = pollster::block_on(rend3_gltf::load_gltf(
            renderer,
            &gltf_data,
            &rend3_gltf::GltfLoadSettings::default(),
            |p| async move { rend3_gltf::filesystem_io_func(&parent_directory, &p).await },
        ))
        .expect("Loading gltf scene");

        // Create a single directional light
        //
        // We need to keep the directional light handle alive.
        let directional_light_handle = renderer.add_directional_light(rend3::types::DirectionalLight {
            color: glam::Vec3::ONE,
            intensity: 5.0,
            // Direction will be normalized
            direction: glam::Vec3::new(-1.0, -4.0, 2.0),
            distance: 400.0,
            resolution: 2048,
        });

        self._directional_light_handle = Some(directional_light_handle);

        let animated_object = AnimatedObject {
            animation_data: rend3_anim::AnimationData::from_gltf_scene(&loaded_scene, &loaded_instance),
            loaded_scene,
            loaded_instance,
            animation_time: 0.0,
            last_frame_time: instant::Instant::now(),
        };

        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/cube_3.gltf"));
        let gltf_data = std::fs::read(&path).unwrap();
        let parent_directory = path.parent().unwrap();
        let (loaded_scene, loaded_instance) = pollster::block_on(rend3_gltf::load_gltf(
            renderer,
            &gltf_data,
            &rend3_gltf::GltfLoadSettings::default(),
            |p| async move { rend3_gltf::filesystem_io_func(&parent_directory, &p).await },
        ))
        .expect("Loading gltf scene");

        let animated_object2 = AnimatedObject {
            animation_data: rend3_anim::AnimationData::from_gltf_scene(&loaded_scene, &loaded_instance),
            loaded_scene,
            loaded_instance,
            animation_time: 0.0,
            last_frame_time: instant::Instant::now(),
        };

        self.animated_objects = vec![animated_object, animated_object2];
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

                self.animated_objects.iter_mut().for_each(|animated_object| {
                    let delta = now.duration_since(animated_object.last_frame_time).as_secs_f32();
                    animated_object.last_frame_time = now;
                    update(renderer, delta, animated_object);
                });

                window.request_redraw();
            }
            // Render!
            rend3_framework::Event::RedrawRequested(_) => {
                // Get a frame
                let frame = surface.unwrap().get_current_texture().unwrap();
                // Ready up the renderer
                let (cmd_bufs, ready) = renderer.ready();

                // Lock the routines
                let pbr_routine = rend3_framework::lock(&routines.pbr);
                let tonemapping_routine = rend3_framework::lock(&routines.tonemapping);

                // Build a rendergraph
                let mut graph = rend3::graph::RenderGraph::new();

                let frame_handle =
                    graph.add_imported_render_target(&frame, 0..1, rend3::graph::ViewportRect::from_size(resolution));
                // Add the default rendergraph without a skybox
                base_rendergraph.add_to_graph(
                    &mut graph,
                    &ready,
                    &pbr_routine,
                    None,
                    &tonemapping_routine,
                    frame_handle,
                    resolution,
                    SAMPLE_COUNT,
                    glam::Vec4::splat(0.15),
                    glam::Vec4::new(0.10, 0.05, 0.10, 1.0), // Nice scene-referred purple
                );

                // Dispatch a render using the built up rendergraph!
                graph.execute(renderer, cmd_bufs, &ready);

                frame.present();
            }
            // Other events we don't care about
            _ => {}
        }
    }
}

#[cfg_attr(target_os = "android", ndk_glue::main(backtrace = "on", logger(level = "debug")))]
pub fn main() {
    let app = AnimationExample::default();
    rend3_framework::start(
        app,
        winit::window::WindowBuilder::new()
            .with_title("animation-example")
            .with_maximized(true),
    );
}
