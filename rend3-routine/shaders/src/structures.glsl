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
    uint material_idx;
    // Work around https://github.com/gfx-rs/naga/issues/1561
    vec3 inv_squared_scale;
};

struct IndirectCall {
    uint vertex_count;
    uint instance_count;
    uint base_index;
    int vertex_offset;
    uint base_instance;
};

#define FLAGS_ALBEDO_ACTIVE       0x0001
#define FLAGS_ALBEDO_BLEND        0x0002
#define FLAGS_ALBEDO_VERTEX_SRGB  0x0004
#define FLAGS_BICOMPONENT_NORMAL  0x0008
#define FLAGS_SWIZZLED_NORMAL     0x0010
#define FLAGS_YDOWN_NORMAL        0x0020
#define FLAGS_AOMR_COMBINED       0x0040
#define FLAGS_AOMR_SWIZZLED_SPLIT 0x0080
#define FLAGS_AOMR_SPLIT          0x0100
#define FLAGS_AOMR_BW_SPLIT       0x0200
#define FLAGS_CC_GLTF_COMBINED    0x0400
#define FLAGS_CC_GLTF_SPLIT       0x0800
#define FLAGS_CC_BW_SPLIT         0x1000
#define FLAGS_UNLIT               0x2000
#define FLAGS_NEAREST             0x4000

#define MATERIAL_FLAG(name) bool(material.material_flags & name)

struct GPUMaterialData {
    uint albedo_tex;
    uint normal_tex;
    uint roughness_tex;
    uint metallic_tex;
    // -- 16 --
    uint reflectance_tex;
    uint clear_coat_tex;
    uint clear_coat_roughness_tex;
    uint emissive_tex;
    // -- 16 --
    uint anisotropy_tex;
    uint ambient_occlusion_tex;
    uint _padding0;
    uint _padding1;
    
    // -- 16 --

    mat3 uv_transform0;
    // -- 16 --
    mat3 uv_transform1;
    // -- 16 --
    vec4 albedo;
    // -- 16 --
    vec3 emissive;
    float roughness;
    // -- 16 --
    float metallic;
    float reflectance;
    float clear_coat;
    float clear_coat_roughness;
    // -- 16 --
    float anisotropy;
    float ambient_occlusion;
    float alpha_cutout;
    uint material_flags;
};

struct CPUMaterialData {
    mat3 uv_transform0;
    // -- 16 --
    mat3 uv_transform1;
    // -- 16 --
    vec4 albedo;
    // -- 16 --
    vec3 emissive;
    float roughness;
    // -- 16 --
    float metallic;
    float reflectance;
    float clear_coat;
    float clear_coat_roughness;
    // -- 16 --
    float anisotropy;
    float ambient_occlusion;
    float alpha_cutout;
    uint material_flags;
    
    // -- 16 --
    uint texture_enable;
};

struct UniformData {
    mat4 view;
    mat4 view_proj;
    mat4 origin_view_proj;
    mat4 inv_view;
    mat4 inv_view_proj;
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
    vec3 direction;
    vec2 offset;
    float size;
};

#endif
