#version 440

#ifdef GPU_DRIVEN
#extension GL_EXT_nonuniform_qualifier : require
#endif

#include "structures.glsl"

layout(location = 0) in vec4 i_view_position;
layout(location = 1) in vec3 i_normal;
layout(location = 2) in vec3 i_tangent;
layout(location = 3) in vec2 i_coords0;
layout(location = 4) in vec2 i_coords1;
layout(location = 5) in vec4 i_color;
layout(location = 6) flat in uint i_material;

layout(location = 0) out vec4 o_color;

layout(set = 0, binding = 0) uniform sampler primary_sampler;
layout(set = 0, binding = 1) uniform sampler nearest_sampler;
layout(set = 0, binding = 2) uniform samplerShadow shadow_sampler;
layout(set = 0, binding = 3) uniform UniformBuffer {
    UniformData uniforms;
};
layout(set = 0, binding = 4) restrict readonly buffer DirectionalLightBuffer {
    DirectionalLightBufferHeader directional_light_header;
    DirectionalLight directional_lights[];
};
layout(set = 0, binding = 5) uniform texture2DArray shadow;
#ifdef GPU_DRIVEN
layout(set = 1, binding = 1, std430) restrict readonly buffer MaterialBuffer {
    GPUMaterialData materials[];
};
layout(set = 2, binding = 0) uniform texture2D textures[];
#endif
#ifdef CPU_DRIVEN
layout(set = 2, binding = 0) readonly buffer TextureData {
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

#include "lighting/surface.glsl"
#include "lighting/pcf.glsl"

void main() {
    #ifdef GPU_DRIVEN
    GPUMaterialData material = materials[i_material];
    #endif

    PixelData pixel = get_per_pixel_data(material);

    if (MATERIAL_FLAG(FLAGS_UNLIT)) {
        o_color = pixel.albedo;
        // o_normal = vec4(i_normal, 0.0);
    } else {
        vec3 v = -normalize(i_view_position.xyz);

        vec3 color = vec3(pixel.emissive);
        for (uint i = 0; i < directional_light_header.total_lights; ++i) {
            DirectionalLight light = directional_lights[i];

            vec3 shadow_ndc = (directional_lights[i].view_proj * uniforms.inv_view * i_view_position).xyz;
            vec2 shadow_flipped = (shadow_ndc.xy * 0.5) + 0.5;
            vec4 shadow_shadow_coords = vec4(shadow_flipped.x, 1 - shadow_flipped.y, float(i), shadow_ndc.z);

            float shadow_value;
            if (shadow_shadow_coords.x < 0 || shadow_shadow_coords.x > 1 || shadow_shadow_coords.y < 0 || shadow_shadow_coords.y > 1 || shadow_ndc.z < -1 || shadow_ndc.z > 1) {
                shadow_value = 1.0;
            } else {
                shadow_value = sample_shadow_pcf5(shadow, shadow_sampler, shadow_shadow_coords);
            }

            color += surface_shading(directional_lights[i], pixel, v, shadow_value * pixel.ambient_occlusion);
        }

        o_color = max(vec4(color, pixel.albedo.a), uniforms.ambient * pixel.albedo);
    }
}
