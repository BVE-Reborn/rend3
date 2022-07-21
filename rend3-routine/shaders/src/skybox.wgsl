{{include "rend3-routine/structures.wgsl"}}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) clip_position: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VertexOutput {
    let clip_position = vec2<f32>(f32(id / 2u) * 4.0 - 1.0, f32(id % 2u) * 4.0 - 1.0);

    return VertexOutput(vec4<f32>(clip_position, 0.0, 1.0), clip_position);
}

@group(0) @binding(0)
var primary_sampler: sampler;
@group(0) @binding(3)
var<uniform> uniforms: UniformData;
@group(1) @binding(0)
var skybox: texture_cube<f32>;

@fragment
fn fs_main(output: VertexOutput) -> @location(0) vec4<f32> {
    // We use the near plane as depth here, as if we used the far plane, it would all NaN out. Doesn't _really_ matter,
    // but 1.0 is a nice round number and results in a depth of 0.1 with my near plane. Good 'nuf.
    let clip = vec4<f32>(output.clip_position, 1.0, 1.0);
    let world_undiv = uniforms.inv_origin_view_proj * clip;
    let world = world_undiv.xyz / world_undiv.w;
    let world_dir = normalize(world);

    let background = textureSample(skybox, primary_sampler, world_dir).rgb;

    return vec4<f32>(background, 1.0);
}
