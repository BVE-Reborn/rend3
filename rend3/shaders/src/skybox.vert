#version 440

layout(location = 0) out vec2 o_clip_position;

void main() {
    uint id = gl_VertexIndex;
    vec2 position = vec2(float(id / 2) * 4.0 - 1.0, float(id % 2) * 4.0 - 1.0);
    o_clip_position = position;

    // We use 0.0 (the infinite far plane) as depth
    gl_Position = vec4(position, 0.0, 1.0);
}