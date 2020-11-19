#version 450

#extension GL_EXT_nonuniform_qualifier : require

#include "structures.glsl"

layout(location = 0) in vec4 i_view_position;
layout(location = 1) in vec3 i_normal;
layout(location = 2) in vec2 i_coords;
layout(location = 3) in vec4 i_color;
layout(location = 4) flat in uint i_material;

layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_normal;

layout(set = 0, binding = 0) uniform sampler linear_sampler;
layout(set = 0, binding = 1) uniform samplerShadow shadow_sampler;
layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
layout(set = 2, binding = 0) uniform MaterialBuffer {
    MaterialData materials[MATERIAL_COUNT];
};
layout(set = 3, binding = 0) uniform texture2D textures[];
layout(set = 4, binding = 0) restrict readonly buffer DirectionalLightBuffer {
    DirectionalLightBufferHeader directional_light_header;
    DirectionalLight directional_lights[];
};
layout(set = 4, binding = 1) uniform texture2DArray shadow;
layout(set = 5, binding = 0) uniform UniformBuffer {
    UniformData uniforms;
};

#include "lighting/surface.glsl"

void main() {
    MaterialData material = materials[i_material];

    PixelData pixel = get_per_pixel_data(material);

    vec3 v = -normalize(i_view_position.xyz);

    vec3 color = vec3(0.0);
    for (uint i = 0; i < directional_light_header.total_lights; ++i) {
        DirectionalLight light = directional_lights[i];

        vec3 shadow_ndc = (directional_lights[i].view_proj * uniforms.inv_view * i_view_position).xyz;
        vec2 shadow_flipped = (shadow_ndc.xy * 0.5) + 0.5;
        vec4 shadow_shadow_coords = vec4(shadow_flipped.x, 1 - shadow_flipped.y, light.shadow_tex, shadow_ndc.z);

        float shadow_value;
        if (shadow_shadow_coords.x < 0 || shadow_shadow_coords.x > 1 || shadow_shadow_coords.y < 0 || shadow_shadow_coords.y > 1) {
            shadow_value = 1.0;
        } else {
            shadow_value = texture(sampler2DArrayShadow(shadow, shadow_sampler), shadow_shadow_coords);
        }

        color += surface_shading(directional_lights[i], pixel, v, shadow_value);
    }

    o_color =  vec4(color, 1.0);
    o_normal = vec4(pixel.normal, 0.0);
}
