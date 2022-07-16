struct Sphere {
    location: vec3<f32>,
    radius: f32,
}

fn sphere_transform_by_mat4(sphere: Sphere, transform: mat4x4<f32>) -> Sphere {
    let length0 = length(transform[0].xyz);
    let length1 = length(transform[1].xyz);
    let length2 = length(transform[2].xyz);
    let max_scale = max(max(length0, length1), length2);
    let center = (transform * vec4<f32>(sphere.location, 1.0)).xyz;
    let radius = sphere.radius * max_scale;

    return Sphere(center, radius);
}
