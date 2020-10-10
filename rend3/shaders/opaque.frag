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

layout(set = 0, binding = 2) uniform MaterialBuffer {
    MaterialData materials[MATERIAL_COUNT];
};
layout(set = 0, binding = 3) uniform sampler linear_sampler;
layout(set = 0, binding = 4) uniform samplerShadow shadow_sampler;
layout(set = 0, binding = 5) restrict readonly buffer DirectionalLightBuffer {
    DirectionalLightBufferHeader directional_light_header;
    DirectionalLight directional_lights[];
};
layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
layout(set = 1, binding = 1) uniform UniformBuffer {
    UniformData uniforms;
};
layout(set = 2, binding = 0) uniform texture2D textures[];
layout(set = 3, binding = 0) uniform texture2D internal_textures[];

#include "lighting/surface.glsl"

void main() {
    MaterialData material = materials[i_material];

    PixelData pixel = get_per_pixel_data(material);

    vec3 v = -normalize(i_view_position.xyz);

    vec3 color = vec3(0.0);
    for (uint i = 0; i < directional_light_header.total_lights; ++i) {
        DirectionalLight light = directional_lights[i];

        vec3 shadow_ndc = (directional_lights[i].view_proj * uniforms.inv_view * i_view_position).xyz;
        vec3 shadow_texture_coords = vec3((shadow_ndc.xy + 1.0) * 0.5, shadow_ndc.z);

        float shadow_value = texture(sampler2DShadow(internal_textures[light.shadow_tex - 1], shadow_sampler), shadow_texture_coords);

        color += surface_shading(directional_lights[i], pixel, v, shadow_value);
    }

    o_color =  vec4(color, 1.0);
    o_normal = vec4(pixel.normal, 0.0);
}
