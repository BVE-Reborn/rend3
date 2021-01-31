#version 450

#ifdef GPU_MODE
#extension GL_EXT_nonuniform_qualifier : require
#endif

#include "structures.glsl"

layout(location = 0) in vec4 i_position;
layout(location = 1) in vec2 i_coords;
layout(location = 2) in vec4 i_color;
layout(location = 3) flat in uint i_material;

layout(set = 0, binding = 0) uniform sampler linear_sampler;
layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
layout(set = 2, binding = 0) uniform UniformBuffer {
    UniformData uniforms;
};
#ifdef GPU_MODE
layout(set = 3, binding = 0, std430) restrict readonly buffer MaterialBuffer {
    GPUMaterialData materials[];
};
layout(set = 4, binding = 0) uniform texture2D textures[];
#endif
#ifdef CPU_MODE
layout(set = 3, binding = 0) uniform texture2D albedo_tex;
layout(set = 3, binding = 1) uniform texture2D normal_tex;
layout(set = 3, binding = 2) uniform texture2D roughness_tex;
layout(set = 3, binding = 3) uniform texture2D metallic_tex;
layout(set = 3, binding = 4) uniform texture2D reflectance_tex;
layout(set = 3, binding = 5) uniform texture2D clear_coat_tex;
layout(set = 3, binding = 6) uniform texture2D clear_coat_roughness_tex;
layout(set = 3, binding = 7) uniform texture2D emissive_tex;
layout(set = 3, binding = 8) uniform texture2D anisotropy_tex;
layout(set = 3, binding = 9) uniform texture2D ambient_occlusion_tex;
layout(set = 3, binding = 10) uniform TextureData {
    CPUMaterialData material;
};
#endif

#include "lighting/texture_access.glsl"

void main() {
    #ifdef GPU_MODE
    GPUMaterialData material = materials[i_material];
    #endif

    bool has_albedo = HAS_ALBEDO_TEXTURE;

    if (has_albedo) {
        vec2 coords = vec2(material.uv_transform * vec3(i_coords, 1.0));
        vec4 albedo = texture(sampler2D(ALBEDO_TEXTURE, linear_sampler), coords);

        if (albedo.a <= 0.5) {
            discard;
        }
    }
}
