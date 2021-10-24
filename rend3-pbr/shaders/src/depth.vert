#version 440

#include "structures.glsl"

// TODO: we don't need most of these
layout(location = 0) in vec3 i_position;
layout(location = 1) in vec3 i_normal;
layout(location = 2) in vec3 i_tangent;
layout(location = 3) in vec2 i_coords0;
layout(location = 4) in vec2 i_coords1;
layout(location = 5) in vec4 i_color;
layout(location = 6) in uint i_material;
#ifdef GPU_MODE
layout(location = 7) in uint i_object_idx;
#endif

layout(location = 0) out vec4 o_position;
layout(location = 1) out vec2 o_coords0;
layout(location = 2) out vec4 o_color;
layout(location = 3) flat out uint o_material;

layout(set = 1, binding = 0, std430) readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
#ifdef GPU_MODE
layout(set = 2, binding = 0, std430) readonly buffer MaterialBuffer {
    GPUMaterialData materials[];
};
#endif
#ifdef CPU_MODE
layout(set = 2, binding = 10) uniform TextureData {
    CPUMaterialData material;
};
#endif

void main() {
    #ifdef GPU_MODE
    GPUMaterialData material = materials[i_material];
    #endif

    #ifdef CPU_MODE
    uint object_idx = gl_InstanceIndex;
    #else
    uint object_idx = i_object_idx;
    #endif

    ObjectOutputData data = object_output[object_idx];

    vec4 position = data.model_view_proj * vec4(i_position, 1.0);
    o_position = position;
    gl_Position = position;

    o_material = data.material_idx;

    o_color = i_color;

    o_coords0 = i_coords0;
}
