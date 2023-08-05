use std::f32::consts::FRAC_PI_2;

use anyhow::Context;
use glam::{Mat4, Vec3, Vec3A};
use rend3::types::{Camera, Handedness};
use rend3_test::{no_gpu_return, test_attr, TestRunner};

#[test_attr]
pub async fn shadows() -> anyhow::Result<()> {
    let iad = no_gpu_return!(rend3::create_iad(None, None, None, None).await)
        .context("InstanceAdapterDevice creation failed")?;

    iad.device.start_capture();

    let Ok(runner) = TestRunner::builder().iad(iad.clone()).handedness(Handedness::Left).build().await else {
            return Ok(());
        };

    let _plane = runner.plane(glam::Vec4::new(0.25, 0.5, 0.75, 1.0), glam::Mat4::IDENTITY);

    runner.set_camera_data(Camera {
        projection: rend3::types::CameraProjection::Orthographic {
            size: Vec3A::splat(4.0),
        },
        view: Mat4::IDENTITY,
    });

    let file_name = "tests/results/shadow/plane.png";
    runner.render_and_compare(64, file_name, 0.0).await?;

    iad.device.stop_capture();

    Ok(())
}
