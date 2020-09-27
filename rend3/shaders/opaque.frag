#version 450

#extension GL_EXT_nonuniform_qualifier : require

#include "structures.glsl"

layout(location = 0) in vec4 i_position;
layout(location = 1) in vec2 i_coords;
layout(location = 2) in vec4 i_color;
layout(location = 3) flat in uint i_material;

layout(location = 0) out vec4 o_color;

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
        vec4 color = texture(sampler2D(textures[nonuniformEXT(color_idx)], samplr), i_coords);

        res *= color;
    }

    o_color = res;
}
