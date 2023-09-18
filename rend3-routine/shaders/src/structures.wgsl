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
    vertex_count: atomic<u32>,
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

struct PerCameraUniformObjectData {
    // TODO: use less space
    model_view: mat4x4<f32>,
    // TODO: use less space
    model_view_proj: mat4x4<f32>,
}

struct PerCameraUniform {
    // TODO: use less space
    view: mat4x4<f32>,
    // TODO: use less space
    view_proj: mat4x4<f32>,
    // The index of which shadow caster we are rendering for.
    //
    // This will be u32::MAX if we're rendering for a camera, not a shadow map.
    shadow_index: u32,
    frustum: Frustum,
    resolution: vec2<f32>,
    // Uses PCU_FLAGS_* constants
    flags: u32,
    object_count: u32,
    objects: array<PerCameraUniformObjectData>,
}

// Area visible
const PCU_FLAGS_AREA_VISIBLE_MASK: u32 = 0x1u;
const PCU_FLAGS_NEGATIVE_AREA_VISIBLE: u32 = 0x0u;
const PCU_FLAGS_POSITIVE_AREA_VISIBLE: u32 = 0x1u;

// Multisampled
const PCU_FLAGS_MULTISAMPLE_MASK: u32 = 0x2u;
const PCU_FLAGS_MULTISAMPLE_DISABLED: u32 = 0x0u;
const PCU_FLAGS_MULTISAMPLE_ENABLED: u32 = 0x2u;

struct DirectionalLight {
    /// View/Projection of directional light. Shadow rendering uses viewports
    /// so this always outputs [-1, 1] no matter where in the atlast the shadow is.
    view_proj: mat4x4<f32>,
    /// Color/intensity of the light
    color: vec3<f32>,
    /// Direction of the light
    direction: vec3<f32>,
    /// 1 / resolution of whole shadow map
    inv_resolution: vec2<f32>,
    /// [0, 1] offset of the shadow map in the atlas.
    offset: vec2<f32>,
    /// [0, 1] size of the shadow map in the atlas.
    size: vec2<f32>,
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