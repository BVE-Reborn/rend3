#version 450

#include "structures.glsl"

layout(location = 0) in vec3 i_position;
layout(location = 1) in vec3 i_normal;
layout(location = 2) in vec2 i_coords;
layout(location = 3) in vec4 i_color;
layout(location = 4) in uint i_material;

layout(location = 0) out vec4 o_position;
layout(location = 1) out vec2 o_coords;
layout(location = 2) out vec4 o_color;
layout(location = 3) flat out uint o_material;

layout(set = 0, binding = 1, std430) restrict readonly buffer MaterialTranslationbuffer {
    uint material_translation[];
};
layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
layout(set = 1, binding = 1) uniform UniformBuffer {
    UniformData uniforms;
};

void main() {
    uint object_idx = gl_InstanceIndex;

    ObjectOutputData data = object_output[object_idx];

    vec4 position = data.model_view_proj * vec4(i_position, 1.0);
    o_position = position;
    gl_Position = position;

    uint material = material_translation[data.material_translation_idx + i_material];
    o_material = material;

    o_color = i_color;

    o_coords = i_coords;
}
