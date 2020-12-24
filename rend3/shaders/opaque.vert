#version 450

#extension GL_GOOGLE_include_directive : require

#include "structures.glsl"

layout(location = 0) in vec3 i_position;
layout(location = 1) in vec3 i_normal;
layout(location = 2) in vec2 i_coords;
layout(location = 3) in vec4 i_color;
layout(location = 4) in uint i_material;
#ifdef GPU_MODE
layout(location = 5) in uint i_object_idx;
#endif

layout(location = 0) out vec4 o_view_position;
layout(location = 1) out vec3 o_normal;
layout(location = 2) out vec2 o_coords;
layout(location = 3) out vec4 o_color;
layout(location = 4) flat out uint o_material;

layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
layout(set = 3, binding = 0) uniform UniformBuffer {
    UniformData uniforms;
};
#ifdef CPU_MODE
layout(push_constant) uniform PushConstant {
    uint i_object_idx;
};
#endif

void main() {
    uint object_idx = i_object_idx;

    ObjectOutputData data = object_output[object_idx];

    gl_Position = data.model_view_proj * vec4(i_position, 1.0);

    o_material = data.material_idx;

    o_view_position = data.model_view * vec4(i_position, 1.0);

    o_normal = data.inv_trans_model_view * i_normal;

    o_color = i_color;

    o_coords = i_coords;
}
