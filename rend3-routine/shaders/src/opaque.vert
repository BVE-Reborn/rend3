#version 440

#extension GL_GOOGLE_include_directive : require

#include "structures.glsl"

layout(location = 0) in vec3 i_position;
layout(location = 1) in vec3 i_normal;
layout(location = 2) in vec3 i_tangent;
layout(location = 3) in vec2 i_coords0;
layout(location = 4) in vec2 i_coords1;
layout(location = 5) in vec4 i_color;
#ifdef GPU_DRIVEN
layout(location = 8) in uint i_object_idx;
#endif

layout(location = 0) out vec4 o_view_position;
layout(location = 1) out vec3 o_normal;
layout(location = 2) out vec3 o_tangent;
layout(location = 3) out vec2 o_coords0;
layout(location = 4) out vec2 o_coords1;
layout(location = 5) out vec4 o_color;
layout(location = 6) flat out uint o_material;

layout(set = 0, binding = 3) uniform UniformBuffer {
    UniformData uniforms;
};
layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
#ifdef GPU_DRIVEN
layout(set = 1, binding = 1, std430) readonly buffer MaterialBuffer {
    GPUMaterialData materials[];
};
#endif
#ifdef CPU_DRIVEN
layout(set = 2, binding = 0) readonly buffer TextureData {
    CPUMaterialData material;
};
#endif

void main() {
    #ifdef CPU_DRIVEN
    uint object_idx = gl_InstanceIndex;
    #else
    uint object_idx = i_object_idx;
    #endif

    ObjectOutputData data = object_output[object_idx];

    o_material = data.material_idx;

    o_view_position = data.model_view * vec4(i_position, 1.0);

    o_normal = normalize(mat3(data.model_view) * (data.inv_squared_scale * i_normal));

    o_tangent = normalize(mat3(data.model_view) * (data.inv_squared_scale * i_tangent));

    o_color = i_color;

    o_coords0 = i_coords0;
    o_coords1 = i_coords1;

    gl_Position = data.model_view_proj * vec4(i_position, 1.0);
}
