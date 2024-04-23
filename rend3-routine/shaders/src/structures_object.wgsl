{{include "rend3-routine/math/sphere.wgsl"}}

struct Object {
    transform: mat4x4<f32>,
    bounding_sphere: Sphere,
    first_index: u32,
    index_count: u32,
    material_index: u32,
    vertex_attribute_start_offsets: array<u32, {{vertex_array_counts}}>,
    // 1 if enabled, 0 if disabled
    enabled: u32,
}
