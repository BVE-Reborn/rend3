use std::sync::Arc;

use glam::{Mat4, Vec3, Vec4};
use rend3::types::{DirectionalLightHandle, MaterialHandle, MeshBuilder, ObjectHandle};
use rend3_routine::pbr::PbrMaterial;
use wgpu::Device;

use crate::TestRunner;

pub struct CaptureDropGuard {
    device: Arc<Device>,
}
impl CaptureDropGuard {
    pub fn start_capture(device: Arc<Device>) -> Self {
        device.start_capture();
        Self { device }
    }
}
impl Drop for CaptureDropGuard {
    fn drop(&mut self) {
        self.device.stop_capture();
        // Wait long enough for the Renderdoc UI to pick up the capture.
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

impl TestRunner {
    pub fn add_directional_light(&self, direction: Vec3) -> DirectionalLightHandle {
        self.renderer.add_directional_light(rend3::types::DirectionalLight {
            color: glam::Vec3::ONE,
            resolution: 256,
            distance: 5.0,
            intensity: 1.0,
            direction,
        })
    }

    pub fn add_unlit_material(&self, color: Vec4) -> MaterialHandle {
        self.renderer.add_material(PbrMaterial {
            albedo: rend3_routine::pbr::AlbedoComponent::Value(color),
            unlit: true,
            ..Default::default()
        })
    }

    pub fn add_lit_material(&self, color: Vec4) -> MaterialHandle {
        self.renderer.add_material(PbrMaterial {
            albedo: rend3_routine::pbr::AlbedoComponent::Value(color),
            unlit: false,
            ..Default::default()
        })
    }

    /// Creates a plane object that is [-1, 1]
    pub fn plane(&self, material: MaterialHandle, transform: Mat4) -> ObjectHandle {
        let mesh = MeshBuilder::new(
            vec![
                glam::Vec3::new(-1.0, -1.0, 0.0),
                glam::Vec3::new(-1.0, 1.0, 0.0),
                glam::Vec3::new(1.0, 1.0, 0.0),
                glam::Vec3::new(1.0, -1.0, 0.0),
            ],
            rend3::types::Handedness::Left,
        )
        .with_indices(vec![0, 2, 1, 0, 3, 2])
        .build()
        .unwrap();

        self.add_object(rend3::types::Object {
            mesh_kind: rend3::types::ObjectMeshKind::Static(self.add_mesh(mesh)),
            material,
            transform,
        })
    }

    /// Creates a cube object that is [-1, 1]
    pub fn cube(&self, material: MaterialHandle, transform: Mat4) -> ObjectHandle {
        let vertex_positions = vec![
            // far side (0.0, 0.0, 1.0)
            glam::Vec3::new(-1.0, -1.0, 1.0),
            glam::Vec3::new(1.0, -1.0, 1.0),
            glam::Vec3::new(1.0, 1.0, 1.0),
            glam::Vec3::new(-1.0, 1.0, 1.0),
            // near side (0.0, 0.0, -1.0)
            glam::Vec3::new(-1.0, 1.0, -1.0),
            glam::Vec3::new(1.0, 1.0, -1.0),
            glam::Vec3::new(1.0, -1.0, -1.0),
            glam::Vec3::new(-1.0, -1.0, -1.0),
            // right side (1.0, 0.0, 0.0)
            glam::Vec3::new(1.0, -1.0, -1.0),
            glam::Vec3::new(1.0, 1.0, -1.0),
            glam::Vec3::new(1.0, 1.0, 1.0),
            glam::Vec3::new(1.0, -1.0, 1.0),
            // left side (-1.0, 0.0, 0.0)
            glam::Vec3::new(-1.0, -1.0, 1.0),
            glam::Vec3::new(-1.0, 1.0, 1.0),
            glam::Vec3::new(-1.0, 1.0, -1.0),
            glam::Vec3::new(-1.0, -1.0, -1.0),
            // top (0.0, 1.0, 0.0)
            glam::Vec3::new(1.0, 1.0, -1.0),
            glam::Vec3::new(-1.0, 1.0, -1.0),
            glam::Vec3::new(-1.0, 1.0, 1.0),
            glam::Vec3::new(1.0, 1.0, 1.0),
            // bottom (0.0, -1.0, 0.0)
            glam::Vec3::new(1.0, -1.0, 1.0),
            glam::Vec3::new(-1.0, -1.0, 1.0),
            glam::Vec3::new(-1.0, -1.0, -1.0),
            glam::Vec3::new(1.0, -1.0, -1.0),
        ];

        let index_data = vec![
            0, 1, 2, 2, 3, 0, // far
            4, 5, 6, 6, 7, 4, // near
            8, 9, 10, 10, 11, 8, // right
            12, 13, 14, 14, 15, 12, // left
            16, 17, 18, 18, 19, 16, // top
            20, 21, 22, 22, 23, 20, // bottom
        ];

        let mesh = MeshBuilder::new(vertex_positions, rend3::types::Handedness::Left)
            .with_indices(index_data)
            .build()
            .unwrap();

        self.add_object(rend3::types::Object {
            mesh_kind: rend3::types::ObjectMeshKind::Static(self.add_mesh(mesh)),
            material,
            transform,
        })
    }
}
