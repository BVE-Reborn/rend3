use std::path::Path;

use rend3_gltf::GltfSceneInstance;

const SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::One;

#[derive(Default)]
pub struct SkinningExample {
    loaded_scene: Option<rend3_gltf::LoadedGltfScene>,
    loaded_instance: Option<rend3_gltf::GltfSceneInstance>,
    directional_light_handle: Option<rend3::types::DirectionalLightHandle>,
    armature: Option<rend3_gltf::Armature>,
    elapsed_time: f32,
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
        let t = self.elapsed_time;
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
    /// Set handedness of coordinate system
    fn get_handedness(&self) -> rend3::types::Handedness {
        rend3::types::Handedness::Left    // default
    }


    fn sample_count(&self) -> rend3::types::SampleCount {
        SAMPLE_COUNT
    }

    fn setup(&mut self, context: rend3_framework::SetupContext<'_>) {
        let view_location = glam::Vec3::new(0.0, 0.0, -10.0);
        let view = glam::Mat4::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0);
        let view = view * glam::Mat4::from_translation(-view_location);

        // Set camera's location
        context.renderer.set_camera_data(rend3::types::Camera {
            projection: rend3::types::CameraProjection::Perspective { vfov: 60.0, near: 0.1 },
            view,
        });

        // Load a gltf model with animation data
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src/skinning/RiggedSimple.glb"));
        let gltf_data = std::fs::read(path).unwrap();
        let parent_directory = path.parent().unwrap();
        let (loaded_scene, loaded_instance) = pollster::block_on(rend3_gltf::load_gltf(
            context.renderer,
            &gltf_data,
            &rend3_gltf::GltfLoadSettings::default(),
            |p| async move { rend3_gltf::filesystem_io_func(&parent_directory, &p).await },
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
        self.directional_light_handle = Some(context.renderer.add_directional_light(rend3::types::DirectionalLight {
            color: glam::Vec3::ONE,
            intensity: 10.0,
            // Direction will be normalized
            direction: glam::Vec3::new(-1.0, -4.0, 2.0),
            distance: 400.0,
            resolution: 2048,
        }));
    }

    fn handle_redraw(&mut self, context: rend3_framework::RedrawContext<'_>) {
        self.update_skeleton(context.renderer);

        // Swap the instruction buffers so that our frame's changes can be processed.
        context.renderer.swap_instruction_buffers();
        // Evaluate our frame's world-change instructions
        let mut eval_output = context.renderer.evaluate_instructions();

        // Lock the routines
        let pbr_routine = rend3_framework::lock(&context.routines.pbr);
        let tonemapping_routine = rend3_framework::lock(&context.routines.tonemapping);

        // Build a rendergraph
        let mut graph = rend3::graph::RenderGraph::new();

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
    let app = SkinningExample::default();
    rend3_framework::start(
        app,
        winit::window::WindowAttributes::default().with_title("skinning-example").with_maximized(true),
    );
}

#[cfg(test)]
#[rend3_test::test_attr]
async fn test() {
    crate::tests::test_app(crate::tests::TestConfiguration {
        app: SkinningExample::default(),
        reference_path: "src/skinning/screenshot.png",
        size: glam::UVec2::new(1280, 720),
        threshold_set: rend3_test::Threshold::Mean(0.01).into(),
    })
    .await
    .unwrap();
}
