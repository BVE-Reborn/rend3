#version 450

#extension GL_EXT_nonuniform_qualifier : require

#include "structures.glsl"
#include "lighting.glsl"

layout(location = 0) in vec4 i_view_position;
layout(location = 1) in vec3 i_normal;
layout(location = 2) in vec2 i_coords;
layout(location = 3) in vec4 i_color;
layout(location = 4) flat in uint i_material;

layout(location = 0) out vec4 o_color;
layout(location = 1) out vec4 o_normal;

layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
layout(set = 2, binding = 0) uniform MaterialBuffer {
    MaterialData materials[MATERIAL_COUNT];
};
layout(set = 3, binding = 0) uniform texture2D textures[];
layout(set = 3, binding = 1) uniform sampler samplr;
layout(set = 4, binding = 0) uniform UniformBuffer {
    UniformData uniforms;
};

void main() {
    MaterialData material = materials[i_material];

    bool has_color = material.color != 0;

    vec4 res = i_color;
    if (has_color) {
        uint color_idx = material.color - 1;
        vec4 diffuse_color = texture(sampler2D(textures[nonuniformEXT(color_idx)], samplr), i_coords);

        vec3 f0 = vec3(0.5, 0.5, 0.5);
        vec3 n = i_normal;
        vec3 v = -normalize(i_view_position.xyz);
        vec3 l = normalize(mat3(uniforms.view) * vec3(1.0, 1.0, 0.0));

        vec3 color = surface_shading(diffuse_color.rgb, n, v, l, f0, 0.0, 1.0);

        res *= vec4(color, 1.0);
    }

    o_color = res;
    o_normal = vec4(i_normal, 0.0);
}
