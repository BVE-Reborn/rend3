use glam::Vec4;
use rend3::types::MaterialHandle;
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
}
