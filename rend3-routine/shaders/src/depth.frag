#version 440

#ifdef GPU_MODE
#extension GL_EXT_nonuniform_qualifier : require
#endif

#include "structures.glsl"

layout(location = 0) in vec4 i_position;
layout(location = 1) in vec2 i_coords0;
layout(location = 2) in vec4 i_color;
layout(location = 3) flat in uint i_material;

#ifdef ALPHA_CUTOUT

layout(set = 0, binding = 0) uniform sampler primary_sampler;
#ifdef GPU_MODE
layout(set = 1, binding = 1, std430) readonly buffer MaterialBuffer {
    GPUMaterialData materials[];
};
layout(set = 2, binding = 0) uniform texture2D textures[];
#endif
#ifdef CPU_MODE
layout(set = 2, binding = 0) uniform TextureData {
    CPUMaterialData material;
};
layout(set = 2, binding = 1) uniform texture2D albedo_tex;
layout(set = 2, binding = 2) uniform texture2D normal_tex;
layout(set = 2, binding = 3) uniform texture2D roughness_tex;
layout(set = 2, binding = 4) uniform texture2D metallic_tex;
layout(set = 2, binding = 5) uniform texture2D reflectance_tex;
layout(set = 2, binding = 6) uniform texture2D clear_coat_tex;
layout(set = 2, binding = 7) uniform texture2D clear_coat_roughness_tex;
layout(set = 2, binding = 8) uniform texture2D emissive_tex;
layout(set = 2, binding = 9) uniform texture2D anisotropy_tex;
layout(set = 2, binding = 10) uniform texture2D ambient_occlusion_tex;
#endif

#include "lighting/texture_access.glsl"

void main() {
    #ifdef GPU_MODE
    GPUMaterialData material = materials[i_material];
    #endif

    bool has_albedo = HAS_ALBEDO_TEXTURE;

    vec2 coords = vec2(material.uv_transform0 * vec3(i_coords0, 1.0));
    vec2 uvdx = dFdx(coords);
    vec2 uvdy = dFdy(coords);

    if (has_albedo) {
        vec4 albedo = textureGrad(sampler2D(ALBEDO_TEXTURE, primary_sampler), coords, uvdx, uvdy);

        if (albedo.a <= material.alpha_cutout) {
            discard;
        }
    }
}
#else // ALPHA_CUTOUT
void main() {}
#endif // ALPHA_CUTOUT
