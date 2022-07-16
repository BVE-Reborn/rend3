{{include "math/sphere.wgsl"}}

struct Plane {
    inner: vec4<f32>,
}

fn plane_distance_to_point(plane: Plane, pint: vec3<f32>) -> f32 {
    return dot(plane.inner.xyz, pint) + plane.inner.w;
}

struct Frustum {
    left: Plane,
    right: Plane,
    top: Plane,
    bottom: Plane,
    near: Plane,
}

fn frustum_contains_sphere(frustum: Frustum, sphere: Sphere) -> bool {
    let neg_radius = -sphere.radius;

    if (!(plane_distance_to_point(frustum.left, sphere.location) >= neg_radius)) {
        return false;
    }
    if (!(plane_distance_to_point(frustum.right, sphere.location) >= neg_radius)) {
        return false;
    }
    if (!(plane_distance_to_point(frustum.top, sphere.location) >= neg_radius)) {
        return false;
    }
    if (!(plane_distance_to_point(frustum.bottom, sphere.location) >= neg_radius)) {
        return false;
    }
    if (!(plane_distance_to_point(frustum.near, sphere.location) >= neg_radius)) {
        return false;
    }

    return true;
}

