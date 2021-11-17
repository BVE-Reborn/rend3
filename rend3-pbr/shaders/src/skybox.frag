#version 440

#include "structures.glsl"

layout(location = 0) in vec2 i_clip_position;
layout(location = 0) out vec4 o_color;

layout(set = 0, binding = 0) uniform sampler primary_sampler;
layout(set = 0, binding = 3) uniform UniformBuffer {
    UniformData uniforms;
};
layout(set = 1, binding = 0) uniform textureCube skybox;

void main() {
    // We use the near plane as depth here, as if we used the far plane, it would all NaN out. Doesn't _really_ matter,
    // but 1.0 is a nice round number and results in a depth of 0.1 with my near plane. Good nuf.
    vec4 clip = vec4(i_clip_position, 1.0, 1.0);
    vec4 world = uniforms.inv_origin_view_proj * clip;
    world.xyz /= world.w;
    vec3 world_dir = normalize(vec3(world));

    vec3 background = texture(samplerCube(skybox, primary_sampler), world_dir).rgb;

    o_color = vec4(background, 1.0);
}