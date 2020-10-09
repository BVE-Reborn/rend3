#ifndef SHADER_STRUCTURES_GLSL
#define SHADER_STRUCTURES_GLSL

struct Plane {
    vec4 inner;
};

struct Frustum {
    Plane left;
    Plane right;
    Plane top;
    Plane bottom;
// No far plane
    Plane near;
};

struct ObjectInputData {
    uint start_idx;
    uint count;
    int vertex_offset;
    uint material_translation_idx;
    mat4 transform;
    // xyz position; w radius
    vec4 bounding_sphere;
};

/// If you change this struct, change the object output size in culling.rs
struct ObjectOutputData {
    mat4 model_view;
    mat4 model_view_proj;
    mat3 inv_trans_model_view;
    uint material_translation_idx;
    bool activ;
};

struct IndirectCall {
    uint vertex_count;
    uint instance_count;
    uint base_index;
    int vertex_offset;
    uint base_instance;
};

#define FLAGS_ALBEDO_ACTIVE      0x01
#define FLAGS_ALBEDO_BLEND       0x02
#define FLAGS_ALBEDO_VERTEX_SRGB 0x04
#define FLAGS_ALPHA_CUTOUT       0x08

struct MaterialData {
    vec4 albedo;
    float roughness;
    float metallic;
    float reflectance;
    float clear_coat;
    float clear_coat_roughness;
    float anisotropy;
    float ambient_occlusion;
    float alpha_cutout;

    uint albedo_tex;
    uint normal_tex;
    uint roughness_tex;
    uint metallic_tex;
    uint reflectance_tex;
    uint clear_coat_tex;
    uint clear_coat_roughness_tex;
    uint anisotropy_tex;
    uint ambient_occlusion_tex;
    uint material_flags;
};

struct UniformData {
    mat4 view;
    mat4 view_proj;
    mat4 inv_view_proj;
    Frustum frustum;
};

struct DirectionalLightBufferHeader {
    uint total_lights;
};

struct DirectionalLight {
    mat4 inv_view_proj;
    vec3 color;
    uint shadow_tex;
    vec3 direction;
};

#endif
