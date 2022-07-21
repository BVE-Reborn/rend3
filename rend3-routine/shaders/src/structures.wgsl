{{include "rend3-routine/math/sphere.wgsl"}}
{{include "rend3-routine/math/frustum.wgsl"}}

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

struct PixelData {
    albedo: vec4<f32>,
    diffuse_color: vec3<f32>,
    roughness: f32,
    normal: vec3<f32>,
    metallic: f32,
    f0: vec3<f32>,
    perceptual_roughness: f32,
    emissive: vec3<f32>,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    clear_coat_perceptual_roughness: f32,
    anisotropy: f32,
    ambient_occlusion: f32,
    material_flags: u32,
}
