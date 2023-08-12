use anyhow::Context;
use glam::{Mat4, Vec3, Vec4};
use rend3::types::{Camera, Handedness, MeshBuilder, Object, ObjectMeshKind, SampleCount};
use rend3_test::{no_gpu_return, test_attr, FrameRenderSettings, TestRunner};

#[test_attr]
pub async fn triangle() -> anyhow::Result<()> {
    let iad = no_gpu_return!(rend3::create_iad(None, None, None, None).await)
        .context("InstanceAdapterDevice creation failed")?;

    let Ok(runner) = TestRunner::builder().iad(iad.clone()).handedness(Handedness::Left).build().await else {
        return Ok(());
    };

    // Clockwise triangle
    let mesh = MeshBuilder::new(
        vec![
            Vec3::new(0.5, -0.5, 0.0),
            Vec3::new(-0.5, -0.5, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
        ],
        Handedness::Left,
    )
    .build()
    .context("Failed to create mesh")?;

    let mesh_hdl = runner.add_mesh(mesh);
    let material_hdl = runner.add_unlit_material(Vec4::new(0.25, 0.5, 0.75, 1.0));
    let object = Object {
        mesh_kind: ObjectMeshKind::Static(mesh_hdl),
        material: material_hdl,
        transform: Mat4::IDENTITY,
    };
    let _object_hdl = runner.add_object(object);

    runner.set_camera_data(Camera {
        projection: rend3::types::CameraProjection::Raw(Mat4::IDENTITY),
        view: Mat4::IDENTITY,
    });

    runner
        .render_and_compare(
            FrameRenderSettings::new().samples(SampleCount::Four),
            "tests/results/msaa/four.png",
            0.0,
        )
        .await?;

    Ok(())
}
