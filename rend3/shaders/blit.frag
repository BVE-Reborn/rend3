#version 450

layout(location = 0) in vec2 tex_coords;

layout(location = 0) out vec4 color;

layout(set = 0, binding = 0) uniform texture2D source;
layout(set = 0, binding = 1) uniform sampler samplr;

void main() {
    color = texture(sampler2D(source, samplr), tex_coords);
}