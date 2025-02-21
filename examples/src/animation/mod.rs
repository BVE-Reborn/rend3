use std::path::Path;

use rend3::types::DirectionalLightHandle;

const SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::One;

/// The application data, can only be obtained at `setup` time, so it's under an
/// Option in the main struct.
pub struct AnimatedObject {
    loaded_scene: rend3_gltf::LoadedGltfScene,
    loaded_instance: rend3_gltf::GltfSceneInstance,
    animation_data: rend3_anim::AnimationData,
    animation_time: f32,
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
    /// Set handedness of coordinate system
    fn get_handedness(&self) -> rend3::types::Handedness {
        rend3::types::Handedness::Left    // default
    }


    fn sample_count(&self) -> rend3::types::SampleCount {
        SAMPLE_COUNT
    }

    fn setup(&mut self, context: rend3_framework::SetupContext<'_>) {
        let view_location = glam::Vec3::new(0.0, -1.5, 5.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0);
        let view = view * glam::Mat4::from_translation(view_location);

        // Set camera's location
        context.renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
            view,
        });

        // Load a gltf model with animation data
        // Needs to be stored somewhere, otherwise all the data gets freed.
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/animation/resources/scene.gltf"));
        let gltf_data = std::fs::read(path).unwrap();
        let parent_directory = path.parent().unwrap();
        let (loaded_scene, loaded_instance) = pollster::block_on(rend3_gltf::load_gltf(
            context.renderer,
            &gltf_data,
            &rend3_gltf::GltfLoadSettings::default(),
            |p| async move { rend3_gltf::filesystem_io_func(&parent_directory, &p).await },
        ))
        .expect("Loading gltf scene");

        // Create a single directional light
        //
        // We need to keep the directional light handle alive.
        let directional_light_handle = context.renderer.add_directional_light(rend3::types::DirectionalLight {
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
        };

        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/animation/resources/cube_3.gltf"));
        let gltf_data = std::fs::read(path).unwrap();
        let parent_directory = path.parent().unwrap();
        let (loaded_scene, loaded_instance) = pollster::block_on(rend3_gltf::load_gltf(
            context.renderer,
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
        };

        self.animated_objects = vec![animated_object, animated_object2];
    }

    fn handle_redraw(&mut self, context: rend3_framework::RedrawContext<'_>) {
        self.animated_objects.iter_mut().for_each(|animated_object| {
            update(context.renderer, context.delta_t_seconds, animated_object);
        });

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

        // Dispatch a render using the built up rendergraph!
        graph.execute(context.renderer, &mut eval_output);
    }
}

pub fn main() {
    let app = AnimationExample::default();
    rend3_framework::start(
        app,
        winit::window::WindowAttributes::default().with_title("animation-example").with_maximized(true),
    );
}

#[cfg(test)]
#[rend3_test::test_attr]
async fn test() {
    crate::tests::test_app(crate::tests::TestConfiguration {
        app: AnimationExample::default(),
        reference_path: "src/animation/screenshot.png",
        size: glam::UVec2::new(1280, 720),
        threshold_set: rend3_test::Threshold::Mean(0.01).into(),
    })
    .await
    .unwrap();
}
