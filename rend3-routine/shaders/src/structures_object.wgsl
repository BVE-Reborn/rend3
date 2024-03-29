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

struct ObjectCullingInformation {
    invocation_start: u32,
    invocation_end: u32,
    object_id: u32,
    region_id: u32,
    region_base_invocation: u32,
    local_region_id: u32,
    previous_global_invocation: u32,
    atomic_capable: u32,
}

struct BatchData {
    total_objects: u32,
    total_invocations: u32,
    batch_base_invocation: u32,
    object_culling_information: array<ObjectCullingInformation, 256>,
}
