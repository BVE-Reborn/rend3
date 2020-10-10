#version 450

#include "lighting/tonemapping.glsl"

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 color;

layout(set = 0, binding = 0) uniform texture2D source;
layout(set = 0, binding = 1) uniform sampler linear_sampler;

void main() {
    vec4 input_color = texture(sampler2D(source, linear_sampler), tex_coords);
    color = vec4(uncharted2_filmic(input_color.rgb), input_color.a);
}