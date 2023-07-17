use anyhow::Context;
use glam::{Mat4, Vec3, Vec4};
use rend3::types::{Camera, Handedness, MeshBuilder, Object, ObjectMeshKind};
use rend3_test::{test_attr, TestRunner};
use wgpu::FrontFace;

#[test_attr]
pub async fn triangle() -> anyhow::Result<()> {
    let tests = [
        (Handedness::Left, FrontFace::Cw, true),
        (Handedness::Left, FrontFace::Ccw, false),
        (Handedness::Right, FrontFace::Cw, false),
        (Handedness::Right, FrontFace::Ccw, true),
    ];

    let iad = rend3::create_iad(None, None, None, None)
        .await
        .context("InstanceAdapterDevice creation failed")?;

    for (handedness, winding, visible) in tests {
        let Ok(runner) = TestRunner::builder().iad(iad.clone()).handedness(handedness).build().await else {
            return Ok(());
        };

        // Clockwise triangle
        let mesh = MeshBuilder::new(
            match winding {
                FrontFace::Ccw => vec![
                    Vec3::new(0.5, -0.5, 0.0),
                    Vec3::new(0.0, 0.5, 0.0),
                    Vec3::new(-0.5, -0.5, 0.0),
                ],
                FrontFace::Cw => vec![
                    Vec3::new(0.5, -0.5, 0.0),
                    Vec3::new(-0.5, -0.5, 0.0),
                    Vec3::new(0.0, 0.5, 0.0),
                ],
            },
            Handedness::Left,
        )
        .build()
        .context("Failed to create mesh")?;

        let mesh_hdl = runner.add_mesh(mesh);
        let material_hdl = runner.add_color_material(Vec4::new(0.25, 0.5, 0.75, 1.0));
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

        let file_name = match visible {
            true => "tests/results/triangle.png",
            false => "tests/results/triangle-backface.png",
        };
        runner.render_and_compare(64, file_name, 0.0).await?;
    }

    Ok(())
}
