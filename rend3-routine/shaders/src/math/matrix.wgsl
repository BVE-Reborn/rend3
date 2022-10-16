fn mat3_inv_scale_squared(transform: mat3x3<f32>) -> vec3<f32> {
    return vec3<f32>(
        1.0 / dot(transform[0].xyz, transform[0].xyz),
        1.0 / dot(transform[1].xyz, transform[1].xyz),
        1.0 / dot(transform[2].xyz, transform[2].xyz)
    );
}
