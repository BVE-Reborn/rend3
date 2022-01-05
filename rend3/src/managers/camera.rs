use crate::types::{Camera, CameraProjection};
use glam::{Mat4, Vec3};
use rend3_types::Handedness;

/// Manages the camera's location and projection settings.
#[derive(Debug, Clone)]
pub struct CameraManager {
    handedness: Handedness,
    orig_view: Mat4,
    proj: Mat4,
    inv_view: Mat4,
    data: Camera,
    aspect_ratio: f32,
}
impl CameraManager {
    /// Builds a new camera, using the given aspect ratio. If no aspect ratio is
    /// given it is assumed that no aspect ratio scaling should be done.
    pub fn new(data: Camera, handedness: Handedness, aspect_ratio: Option<f32>) -> Self {
        profiling::scope!("CameraManager::new");

        let aspect_ratio = aspect_ratio.unwrap_or(1.0);
        let proj = compute_projection_matrix(data, handedness, aspect_ratio);
        let orig_view = compute_origin_matrix(data);

        Self {
            handedness,
            orig_view,
            proj,
            inv_view: data.view.inverse(),
            data,
            aspect_ratio,
        }
    }

    /// Sets the camera data, rebuilding the using the given aspect ratio. If no
    /// aspect ratio is given it is assumed that no aspect ratio scaling
    /// should be done.
    pub fn set_data(&mut self, data: Camera) {
        self.set_aspect_data(data, self.aspect_ratio)
    }

    pub fn set_aspect_ratio(&mut self, aspect_ratio: Option<f32>) {
        self.set_aspect_data(self.data, aspect_ratio.unwrap_or(1.0));
    }

    pub fn set_aspect_data(&mut self, data: Camera, aspect_ratio: f32) {
        self.proj = compute_projection_matrix(data, self.handedness, aspect_ratio);
        self.orig_view = compute_origin_matrix(data);
        self.inv_view = data.view.inverse();
        self.data = data;
        self.aspect_ratio = aspect_ratio;
    }

    pub fn get_data(&self) -> Camera {
        self.data
    }

    pub fn handedness(&self) -> Handedness {
        self.handedness
    }

    pub fn view(&self) -> Mat4 {
        self.data.view
    }

    pub fn view_proj(&self) -> Mat4 {
        self.proj * self.data.view
    }

    pub fn origin_view_proj(&self) -> Mat4 {
        self.proj * self.orig_view
    }

    pub fn proj(&self) -> Mat4 {
        self.proj
    }

    pub fn location(&self) -> Vec3 {
        self.inv_view.w_axis.truncate()
    }
}

fn compute_projection_matrix(data: Camera, handedness: Handedness, aspect_ratio: f32) -> Mat4 {
    match data.projection {
        CameraProjection::Orthographic { size } => {
            let half = size * 0.5;
            if handedness == Handedness::Left {
                Mat4::orthographic_lh(-half.x, half.x, -half.y, half.y, half.z, -half.z)
            } else {
                Mat4::orthographic_rh(-half.x, half.x, -half.y, half.y, half.z, -half.z)
            }
        }
        CameraProjection::Perspective { vfov, near } => {
            if handedness == Handedness::Left {
                Mat4::perspective_infinite_reverse_lh(vfov.to_radians(), aspect_ratio, near)
            } else {
                Mat4::perspective_infinite_reverse_rh(vfov.to_radians(), aspect_ratio, near)
            }
        }
        CameraProjection::Raw(proj) => proj,
    }
}

fn compute_origin_matrix(data: Camera) -> Mat4 {
    let mut view = data.view;

    view.w_axis = glam::Vec4::W;
    view
}
