use glam::{Mat4, Vec4};
use rend3::types::{MaterialHandle, MeshBuilder, ObjectHandle};
use rend3_routine::pbr::PbrMaterial;

use crate::TestRunner;

impl TestRunner {
    pub fn add_color_material(&self, color: Vec4) -> MaterialHandle {
        self.renderer.add_material(PbrMaterial {
            albedo: rend3_routine::pbr::AlbedoComponent::Value(color),
            unlit: true,
            ..Default::default()
        })
    }

    /// Creates a plane object that is [-1, 1]
    pub fn plane(&self, color: Vec4, transform: Mat4) -> ObjectHandle {
        let mesh = MeshBuilder::new(
            vec![
                glam::Vec3::new(-1.0, 0.0, -1.0),
                glam::Vec3::new(-1.0, 0.0, 1.0),
                glam::Vec3::new(1.0, 0.0, 1.0),
                glam::Vec3::new(1.0, 0.0, -1.0),
            ],
            rend3::types::Handedness::Left,
        )
        .with_indices(vec![0, 1, 2, 0, 2, 3])
        .build()
        .unwrap();

        self.add_object(rend3::types::Object {
            mesh_kind: rend3::types::ObjectMeshKind::Static(self.add_mesh(mesh)),
            material: self.add_color_material(color),
            transform,
        })
    }
}
