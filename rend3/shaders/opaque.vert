#version 450

#extension GL_ARB_shader_draw_parameters : require

#include "structures.glsl"

layout(location = 0) in vec3 i_position;
layout(location = 1) in vec3 i_normal;
layout(location = 2) in vec2 i_coords;
layout(location = 3) in vec4 i_color;
layout(location = 4) in uint i_material;

layout(location = 0) out vec4 o_position;
layout(location = 1) out vec3 o_normal;
layout(location = 2) out vec2 o_coords;
layout(location = 3) out vec4 o_color;
layout(location = 4) flat out uint o_material;

layout(set = 0, binding = 1, std430) restrict readonly buffer MaterialTranslationbuffer {
    uint material_translation[];
};
layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
layout(set = 4, binding = 0) uniform UniformBuffer {
    UniformData uniforms;
};

void main() {
    uint object_idx = gl_DrawIDARB;

    ObjectOutputData data = object_output[object_idx];

    vec4 position = data.model_view_proj * vec4(i_position, 1.0);
    o_position = position;
    gl_Position = position;

    uint material = material_translation[data.material_translation_idx + i_material];
    o_material = material;

    o_normal = data.inv_trans_model_view * i_normal;

    o_color = i_color;

    o_coords = i_coords;
}
