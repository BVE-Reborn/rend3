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
layout(set = 0, binding = 3) uniform sampler samplr;
layout(set = 1, binding = 0, std430) restrict readonly buffer ObjectOutputDataBuffer {
    ObjectOutputData object_output[];
};
layout(set = 1, binding = 1) uniform UniformBuffer {
    UniformData uniforms;
};
layout(set = 2, binding = 0) uniform texture2D textures[];

#include "lighting/surface.glsl"

void main() {
    MaterialData material = materials[i_material];

    PixelData pixel = get_per_pixel_data(material);

    vec3 v = -normalize(i_view_position.xyz);
    vec3 l = normalize(mat3(uniforms.view) * vec3(1.0, 1.0, 0.0));

    vec3 color = surface_shading(pixel, v, l, pixel.ambient_occlusion);

    o_color =  vec4(color, 1.0);
    o_normal = vec4(pixel.normal, 0.0);
}
