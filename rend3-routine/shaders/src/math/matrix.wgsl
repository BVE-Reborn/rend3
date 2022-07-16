fn mat3_inv_scale_squared(_matrix: mat3x3<f32>) -> vec3<f32> {
    return vec3<f32>(
        1.0 / dot(_matrix[0].xyz, _matrix[0].xyz),
        1.0 / dot(_matrix[1].xyz, _matrix[1].xyz),
        1.0 / dot(_matrix[2].xyz, _matrix[2].xyz)
    );
}
