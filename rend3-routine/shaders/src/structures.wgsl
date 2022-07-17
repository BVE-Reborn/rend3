{{include "math/sphere.wgsl"}}
{{include "math/frustum.wgsl"}}

struct ObjectInputData {
    start_idx: u32,
    count: u32,
    vertex_offset: i32,
    material_idx: u32,
    transform: mat4x4<f32>,
    bounding_sphere: Sphere,
}

struct ObjectOutputData {
    model_view: mat4x4<f32>,
    model_view_proj: mat4x4<f32>,
    material_idx: u32,
    inv_scale_sq: vec3<f32>,
}

struct IndirectCall {
    vertex_count: u32,
    instance_count: u32,
    base_index: u32,
    vertex_offset: i32,
    base_instance: u32,
}

struct UniformData {
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    origin_view_proj: mat4x4<f32>,
    inv_view: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    inv_origin_view_proj: mat4x4<f32>,
    frustum: Frustum,
    ambient: vec4<f32>,
    resolution: vec2<u32>,
}

struct DirectionalLight {
    view_proj: mat4x4<f32>,
    color: vec3<f32>,
    direction: vec3<f32>,
    offset: vec2<f32>,
    size: f32,
}

struct DirectionalLightData {
    count: u32,
    data: array<DirectionalLight>,
}

