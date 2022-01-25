//! Frustums and bounding spheres.
//!
//! This entire module only exists because of <https://www.gamedevs.org/uploads/fast-extraction-viewing-frustum-planes-from-world-view-projection-matrix.pdf>.

use glam::{Mat4, Vec3, Vec3A, Vec4Swizzles};

/// Represents a point in space and a radius from that point.
#[derive(Debug, Clone, Copy)]
#[repr(C, align(16))]
pub struct BoundingSphere {
    pub center: Vec3,
    pub radius: f32,
}
impl BoundingSphere {
    pub fn from_mesh(mesh: &[Vec3]) -> Self {
        let center = find_mesh_center(mesh);
        let radius = find_mesh_bounding_sphere_radius(center, mesh);

        Self {
            center: Vec3::from(center),
            radius,
        }
    }

    pub fn apply_transform(self, model_view: Mat4) -> Self {
        let max_scale = model_view
            .x_axis
            .xyz()
            .length_squared()
            .max(
                model_view
                    .y_axis
                    .xyz()
                    .length_squared()
                    .max(model_view.z_axis.xyz().length_squared()),
            )
            .sqrt();
        let center = model_view * self.center.extend(1.0);

        Self {
            center: center.truncate(),
            radius: max_scale * self.radius,
        }
    }
}

fn find_mesh_center(mesh: &[Vec3]) -> Vec3A {
    let first = if let Some(first) = mesh.first() {
        *first
    } else {
        return Vec3A::ZERO;
    };
    // Bounding box time baby!
    let mut max = Vec3A::from(first);
    let mut min = max;

    for pos in mesh.iter().skip(1) {
        let pos = Vec3A::from(*pos);
        max = max.max(pos);
        min = min.min(pos);
    }

    (max + min) / 2.0
}

fn find_mesh_bounding_sphere_radius(mesh_center: Vec3A, mesh: &[Vec3]) -> f32 {
    mesh.iter().fold(0.0, |distance, pos| {
        distance.max((Vec3A::from(*pos) - mesh_center).length())
    })
}

/// Represents a plane as a vec4 (or vec3 + f32)
#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
pub struct ShaderPlane {
    pub abc: Vec3,
    pub d: f32,
}

impl ShaderPlane {
    pub fn new(a: f32, b: f32, c: f32, d: f32) -> Self {
        Self {
            abc: Vec3::new(a, b, c),
            d,
        }
    }

    pub fn normalize(mut self) -> Self {
        let mag = self.abc.length();

        self.abc /= mag;
        self.d /= mag;

        self
    }

    pub fn distance(self, point: Vec3) -> f32 {
        self.abc.dot(point) + self.d
    }
}

/// A frustum composed of 5 different planes. Has no far plane as it assumes
/// infinite.
#[derive(Debug, Copy, Clone)]
#[repr(C, align(16))]
pub struct ShaderFrustum {
    left: ShaderPlane,
    right: ShaderPlane,
    top: ShaderPlane,
    bottom: ShaderPlane,
    near: ShaderPlane,
}

impl ShaderFrustum {
    pub fn from_matrix(matrix: Mat4) -> Self {
        let mat_arr = matrix.to_cols_array_2d();

        let left = ShaderPlane::new(
            mat_arr[0][3] + mat_arr[0][0],
            mat_arr[1][3] + mat_arr[1][0],
            mat_arr[2][3] + mat_arr[2][0],
            mat_arr[3][3] + mat_arr[3][0],
        );

        let right = ShaderPlane::new(
            mat_arr[0][3] - mat_arr[0][0],
            mat_arr[1][3] - mat_arr[1][0],
            mat_arr[2][3] - mat_arr[2][0],
            mat_arr[3][3] - mat_arr[3][0],
        );

        let top = ShaderPlane::new(
            mat_arr[0][3] - mat_arr[0][1],
            mat_arr[1][3] - mat_arr[1][1],
            mat_arr[2][3] - mat_arr[2][1],
            mat_arr[3][3] - mat_arr[3][1],
        );

        let bottom = ShaderPlane::new(
            mat_arr[0][3] + mat_arr[0][1],
            mat_arr[1][3] + mat_arr[1][1],
            mat_arr[2][3] + mat_arr[2][1],
            mat_arr[3][3] + mat_arr[3][1],
        );

        // no far plane as we have infinite depth

        // this is the far plane in the algorithm, but we're using inverse Z, so near
        // and far get flipped.
        let near = ShaderPlane::new(
            mat_arr[0][3] - mat_arr[0][2],
            mat_arr[1][3] - mat_arr[1][2],
            mat_arr[2][3] - mat_arr[2][2],
            mat_arr[3][3] - mat_arr[3][2],
        );

        Self {
            left: left.normalize(),
            right: right.normalize(),
            top: top.normalize(),
            bottom: bottom.normalize(),
            near: near.normalize(),
        }
    }

    /// Determins if the sphere is at all inside the frustum.
    pub fn contains_sphere(&self, sphere: BoundingSphere) -> bool {
        let neg_radius = -sphere.radius;

        let array = [self.left, self.right, self.top, self.bottom, self.near];

        for plane in &array {
            let inside = plane.distance(sphere.center) >= neg_radius;
            if !inside {
                return false;
            }
        }

        true
    }
}
