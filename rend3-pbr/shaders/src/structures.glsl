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
    uint material_idx;
    mat4 transform;
    // xyz position; w radius
    vec4 bounding_sphere;
}; 

/// If you change this struct, change the object output size in culling.rs
struct ObjectOutputData {
    mat4 model_view;
    mat4 model_view_proj;
    mat3 inv_trans_model_view;
    uint material_idx;
};

struct IndirectCall {
    uint vertex_count;
    uint instance_count;
    uint base_index;
    int vertex_offset;
    uint base_instance;
};

#define FLAGS_ALBEDO_ACTIVE      0x0001
#define FLAGS_ALBEDO_BLEND       0x0002
#define FLAGS_ALBEDO_VERTEX_SRGB 0x0004
#define FLAGS_BICOMPONENT_NORMAL 0x0010
#define FLAGS_SWIZZLED_NORMAL    0x0020
#define FLAGS_AOMR_GLTF_COMBINED 0x0040
#define FLAGS_AOMR_GLTF_SPLIT    0x0080
#define FLAGS_AOMR_BW_SPLIT      0x0100
#define FLAGS_CC_GLTF_COMBINED   0x0200
#define FLAGS_CC_GLTF_SPLIT      0x0400
#define FLAGS_CC_BW_SPLIT        0x0800
#define FLAGS_UNLIT              0x1000
#define FLAGS_NEAREST            0x2000

#define MATERIAL_FLAG(name) bool(material.material_flags & name)

struct GPUMaterialData {
    vec4 albedo;
    vec3 emissive;
    float roughness;
    float metallic;
    float reflectance;
    float clear_coat;
    float clear_coat_roughness;
    float anisotropy;
    float ambient_occlusion;
    float alpha_cutout;

    mat3 uv_transform;

    uint albedo_tex;
    uint normal_tex;
    uint roughness_tex;
    uint metallic_tex;
    uint reflectance_tex;
    uint clear_coat_tex;
    uint clear_coat_roughness_tex;
    uint emissive_tex;
    uint anisotropy_tex;
    uint ambient_occlusion_tex;
    uint material_flags;
};

struct CPUMaterialData {
    // Must be one of the first two members or else spirv-cross can't allocate registers on DX
    mat3 uv_transform;
    vec4 albedo;
    vec3 emissive;
    float roughness;
    float metallic;
    float reflectance;
    float clear_coat;
    float clear_coat_roughness;
    float anisotropy;
    float ambient_occlusion;
    float alpha_cutout;

    uint texture_enable;
    uint material_flags;
};

struct UniformData {
    mat4 view;
    mat4 view_proj;
    mat4 inv_view;
    mat4 inv_origin_view_proj;
    Frustum frustum;
    vec4 ambient;
};

struct DirectionalLightBufferHeader {
    uint total_lights;
};

struct DirectionalLight {
    mat4 view_proj;
    vec3 color;
    uint shadow_tex;
    vec3 direction;
};

#endif
