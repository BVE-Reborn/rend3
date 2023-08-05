use std::{f32::consts::FRAC_PI_2, time::Duration};

use anyhow::Context;
use glam::{Mat4, Quat, Vec3, Vec3A, Vec4};
use rend3::types::{Camera, Handedness};
use rend3_test::{no_gpu_return, test_attr, TestRunner};

#[test_attr]
pub async fn shadows() -> anyhow::Result<()> {
    let iad = no_gpu_return!(rend3::create_iad(None, None, None, None).await)
        .context("InstanceAdapterDevice creation failed")?;

    let Ok(runner) = TestRunner::builder().iad(iad.clone()).handedness(Handedness::Left).build().await else {
        return Ok(());
    };

    let _plane = runner.plane(Vec4::new(0.25, 0.5, 0.75, 1.0), Mat4::from_rotation_x(FRAC_PI_2));

    runner.set_camera_data(Camera {
        projection: rend3::types::CameraProjection::Orthographic {
            size: Vec3A::splat(4.0),
        },
        view: Mat4::look_at_lh(Vec3::new(0.0, 1.0, -1.0), Vec3::ZERO, Vec3::Y),
    });

    let file_name = "tests/results/shadow/plane.png";
    runner.render_and_compare(256, file_name, 0.0).await?;

    let _cube = runner.cube(
        Vec4::new(0.75, 0.5, 0.25, 1.0),
        Mat4::from_scale_rotation_translation(Vec3::splat(0.25), Quat::IDENTITY, Vec3::new(0.25, 0.75, 0.0)),
    );

    let file_name = "tests/results/shadow/cube.png";
    runner.render_and_compare(256, file_name, 0.0).await?;

    Ok(())
}
