use crate::datatypes::CameraLocation;
use glam::{Mat3, Mat4, Vec3, Vec3A};

const CAMERA_VFOV: f32 = 60.0;

#[derive(Copy, Clone)]
pub struct Camera {
    orig_view: Mat4,
    view: Mat4,
    proj: Mat4,
}
impl Camera {
    pub fn new_projection(aspect_ratio: f32) -> Self {
        let proj = Mat4::perspective_infinite_reverse_lh(CAMERA_VFOV.to_radians(), aspect_ratio, 0.1);
        let view = compute_view_matrix(CameraLocation::default());
        let orig_view = compute_origin_matrix(CameraLocation::default());

        Self { orig_view, view, proj }
    }

    pub fn new_orthographic(direction: Vec3) -> Self {
        let proj = Mat4::orthographic_lh(-50.0, 50.0, -50.0, 50.0, -100.0, 100.0);
        let view = Mat4::look_at_lh(Vec3::zero(), direction, Vec3::unit_y());

        Self {
            orig_view: view,
            view,
            proj,
        }
    }

    pub fn set_location(&mut self, location: CameraLocation) {
        self.view = compute_view_matrix(location);
        self.orig_view = compute_origin_matrix(location);
    }

    pub fn set_orthographic_location(&mut self, direction: Vec3) {
        self.view = Mat4::look_at_lh(Vec3::zero(), direction, Vec3::unit_y());
        self.orig_view = self.view;
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

    pub fn origin_view_proj(&self) -> Mat4 {
        self.proj * self.orig_view
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

// This is horribly inefficient but is called like once a frame.
pub fn compute_origin_matrix(location: CameraLocation) -> Mat4 {
    let look_offset = compute_look_offset(location);

    Mat4::look_at_lh(Vec3::zero(), Vec3::from(look_offset), Vec3::unit_y())
}
