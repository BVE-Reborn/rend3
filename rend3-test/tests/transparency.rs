use anyhow::Context;
use glam::{Mat4, Quat, Vec3, Vec4};
use rend3::types::{Camera, Handedness};
use rend3_test::{no_gpu_return, test_attr, FrameRenderSettings, TestRunner, Threshold};

/// Ensure that transparency is ordered correctly
//
// Todo: This test never fails, even if I remove the sort.
#[test_attr]
pub async fn transparency() -> anyhow::Result<()> {
    let iad = no_gpu_return!(rend3::create_iad(None, None, None, None).await)
        .context("InstanceAdapterDevice creation failed")?;

    let Ok(runner) = TestRunner::builder().iad(iad.clone()).handedness(Handedness::Left).build().await else {
        return Ok(());
    };

    runner.set_camera_data(Camera {
        projection: rend3::types::CameraProjection::Raw(Mat4::IDENTITY),
        view: Mat4::IDENTITY,
    });

    let material1 = runner.add_transparent_material(Vec4::new(1.0, 0.0, 0.0, 0.5));
    let material2 = runner.add_transparent_material(Vec4::new(0.0, 1.0, 0.0, 0.5));
    let _object_left_1 = runner.plane(
        material1.clone(),
        Mat4::from_scale_rotation_translation(Vec3::new(-0.25, 0.25, 0.25), Quat::IDENTITY, Vec3::new(-0.5, 0.0, -0.5)),
    );

    let _object_left_2 = runner.plane(
        material2.clone(),
        Mat4::from_scale_rotation_translation(Vec3::new(-0.25, 0.25, 0.25), Quat::IDENTITY, Vec3::new(-0.5, 0.0, 0.5)),
    );

    let _object_right_2 = runner.plane(
        material2,
        Mat4::from_scale_rotation_translation(Vec3::new(-0.25, 0.25, 0.25), Quat::IDENTITY, Vec3::new(0.5, 0.0, 0.5)),
    );

    let _object_right_1 = runner.plane(
        material1,
        Mat4::from_scale_rotation_translation(Vec3::new(-0.25, 0.25, 0.25), Quat::IDENTITY, Vec3::new(0.5, 0.0, -0.5)),
    );

    runner
        .render_and_compare(FrameRenderSettings::new(), "tests/results/transparency/blending.png", Threshold::Mean(0.0))
        .await?;

    Ok(())
}
