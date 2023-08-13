use anyhow::Context;
use glam::{Mat4, Vec3, Vec4};
use rend3::{
    types::{Camera, Handedness},
    util::freelist::FreelistDerivedBuffer,
};
use rend3_test::{no_gpu_return, test_attr, FrameRenderSettings, TestRunner, Threshold};

/// There was a bug in the culling implementation where the per-material buffer
/// was never resized to fit the number of objects in the scene once it was initially
/// created. This manifested as objects above the initial frame count would get all-zero
/// transforms and be completely hidden. We reproduce those conditions here, and ensure
/// that the bug is fixed.
#[test_attr]
pub async fn multi_frame_add() -> anyhow::Result<()> {
    let iad = no_gpu_return!(rend3::create_iad(None, None, None, None).await)
        .context("InstanceAdapterDevice creation failed")?;

    let Ok(runner) = TestRunner::builder().iad(iad.clone()).handedness(Handedness::Left).build().await else {
        return Ok(());
    };

    let material = runner.add_unlit_material(Vec4::ONE);

    // Make a plane whose (0, 0) is at the top left, and is 1 unit large.
    let base_matrix = Mat4::from_translation(Vec3::new(0.5, 0.5, 0.0)) * Mat4::from_scale(Vec3::new(0.5, 1.0, 1.0));

    runner.set_camera_data(Camera {
        projection: rend3::types::CameraProjection::Raw(Mat4::orthographic_lh(0.0, 2.0, 16.0, 0.0, 0.0, 1.0)),
        view: Mat4::IDENTITY,
    });

    // We use the starting size amount of objects for each column, ensuring that the buffer
    // will need to be resized on the second column.
    let count = FreelistDerivedBuffer::STARTING_SIZE;

    // 2 side by side columns made up of `count` rows
    let mut planes = Vec::with_capacity(2);
    for x in 0..2 {
        for y in 0..count {
            planes.push(runner.plane(
                material.clone(),
                Mat4::from_translation(Vec3::new(x as f32, y as f32, 0.0)) * base_matrix,
            ));
        }
        runner
            .render_and_compare(
                FrameRenderSettings::new(),
                &format!("tests/results/object/multi-frame-add-{}.png", x),
                Threshold::Mean(0.0),
            )
            .await?;
    }

    Ok(())
}
