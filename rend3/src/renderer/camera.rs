use crate::datatypes::CameraLocation;
use glam::{Mat3, Mat4, Vec3, Vec3A};

const CAMERA_VFOV: f32 = 60.0;

pub struct Camera {
    view: Mat4,
    proj: Mat4,
}
impl Camera {
    pub fn new(aspect_ratio: f32) -> Self {
        let proj = Mat4::perspective_infinite_reverse_lh(CAMERA_VFOV.to_radians(), aspect_ratio, 0.1);
        let view = compute_view_matrix(CameraLocation::default());

        Self { view, proj }
    }

    pub fn set_location(&mut self, location: CameraLocation) {
        self.view = compute_view_matrix(location);
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.proj = Mat4::perspective_infinite_reverse_lh(CAMERA_VFOV.to_radians(), aspect_ratio, 0.1);
    }

    pub fn view(&self) -> Mat4 {
        self.view
    }

    pub fn view_proj(&self) -> Mat4 {
        self.proj * self.view
    }

    pub fn proj(&self) -> Mat4 {
        self.proj
    }
}

fn compute_look_offset(location: CameraLocation) -> Vec3A {
    let starting = Vec3A::unit_z();
    Mat3::from_rotation_ypr(location.yaw, location.pitch, 0.0) * starting
}

fn compute_view_matrix(location: CameraLocation) -> Mat4 {
    let look_offset = compute_look_offset(location);

    Mat4::look_at_lh(
        Vec3::from(location.location),
        Vec3::from(location.location + look_offset),
        Vec3::unit_y(),
    )
}
