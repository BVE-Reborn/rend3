use glam::{Mat4, Vec3};

pub struct Camera {
    view: Mat4,
    proj: Mat4,
}
impl Camera {
    pub fn new(aspect_ratio: f32) -> Self {
        let proj = Mat4::perspective_infinite_reverse_lh(60.0_f32.to_radians(), aspect_ratio, 0.1);
        let view = Mat4::look_at_lh(Vec3::new(0.0, 0.0, -10.0), Vec3::zero(), Vec3::unit_y());

        Self { view, proj }
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.proj = Mat4::perspective_infinite_reverse_lh(60.0_f32.to_radians(), aspect_ratio, 0.1);
    }

    pub fn view(&self) -> Mat4 {
        self.view
    }

    pub fn view_proj(&self) -> Mat4 {
        self.proj * self.view
    }
}
