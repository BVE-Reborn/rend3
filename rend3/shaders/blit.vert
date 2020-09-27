#version 450

layout(location = 0) out vec2 tex_coords;

void main() {
    uint id = gl_VertexIndex;
    gl_Position = vec4(float(id / 2) * 4.0 - 1.0, float(id % 2) * 4.0 - 1.0, 0.0, 1.0);
    tex_coords = vec2(float(id / 2) * 2.0,  1.0 - (float(id % 2) * 2.0));
}