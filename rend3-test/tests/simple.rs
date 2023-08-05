use anyhow::Context;
use glam::{Mat4, Vec3, Vec4};
use rend3::types::{Camera, Handedness, MeshBuilder, Object, ObjectMeshKind};
use rend3_test::{no_gpu_return, test_attr, TestRunner};
use wgpu::FrontFace;

#[test_attr]
pub async fn triangle() -> anyhow::Result<()> {
    let tests = [
        (Handedness::Left, FrontFace::Cw, true),
        (Handedness::Left, FrontFace::Ccw, false),
        (Handedness::Right, FrontFace::Cw, false),
        (Handedness::Right, FrontFace::Ccw, true),
    ];

    let iad = no_gpu_return!(rend3::create_iad(None, None, None, None).await)
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
            true => "tests/results/simple/triangle.png",
            false => "tests/results/simple/triangle-backface.png",
        };
        runner.render_and_compare(64, file_name, 0.0).await?;
    }

    Ok(())
}

#[test_attr]
pub async fn coordinate_space() -> anyhow::Result<()> {
    let tests = [
        // Right vector, up vector, camera vector
        ("NegZ", Vec3::X, Vec3::Y, -Vec3::Z),
        ("Z", -Vec3::X, Vec3::Y, Vec3::Z),
        ("NegY", Vec3::X, -Vec3::Z, -Vec3::Y),
        ("Y", Vec3::X, Vec3::Z, Vec3::Y),
        ("NegX", -Vec3::Z, Vec3::Y, -Vec3::X),
        ("X", Vec3::Z, Vec3::Y, Vec3::X),
    ];

    let iad = no_gpu_return!(rend3::create_iad(None, None, None, None).await)
        .context("InstanceAdapterDevice creation failed")?;

    let Ok(runner) = TestRunner::builder().iad(iad.clone()).handedness(Handedness::Left).build().await else {
            return Ok(());
        };

    let _objects = tests.map(|(_name, right_vector, up_vector, camera_vector)| {
        // Clockwise triangle
        let mesh = MeshBuilder::new(
            vec![
                0.5 * right_vector + -0.5 * up_vector,
                -0.5 * right_vector + -0.5 * up_vector,
                0.0 * right_vector + 0.5 * up_vector,
            ],
            Handedness::Left,
        )
        .build()
        .expect("Failed to create mesh");

        let color = if camera_vector.is_negative_bitmask() != 0 {
            camera_vector * -0.25
        } else {
            camera_vector
        };

        let mesh_hdl = runner.add_mesh(mesh);
        let material_hdl = runner.add_color_material(color.extend(1.0));
        let object = Object {
            mesh_kind: ObjectMeshKind::Static(mesh_hdl),
            material: material_hdl,
            transform: Mat4::IDENTITY,
        };
        runner.add_object(object)
    });

    for (name, _right_vector, up_vector, camera_vector) in tests {
        runner.set_camera_data(Camera {
            projection: rend3::types::CameraProjection::Raw(Mat4::IDENTITY),
            view: Mat4::look_at_lh(camera_vector, Vec3::ZERO, up_vector),
        });

        let file_name = format!("tests/results/simple/coordinate-space-{name}.png");
        runner.render_and_compare(64, file_name, 0.0).await?;
    }

    Ok(())
}
