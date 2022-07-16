{{include "math/color.wgsl"}}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VertexOutput {
    var output: VertexOutput;
    output.position = vec4<f32>(f32(id / 2) * 4.0 - 1.0, float(id % 2) * 4.0 - 1.0, 0.0, 1.0);
    output.tex_coords = vec2<f32>(f32(id / 2) * 2.0, 1.0 - (f32(id % 2) * 2.0));
    return output;
}

@group(0) @binding(0)
var primary_sampler: sampler;
@group(1) @binding(0)
var source: texture_2d<f32>;

@fragment
fn fs_main_scene(vout: VertexOutput) -> @location(0) vec4<f32> {
    var sampled = textureSample(source, sampler, vout.tex_coords);
    return sampled;
}

@fragment
fn fs_main_monitor(vout: VertexOutput) -> @location(0) vec4<f32> {
    var sampled = textureSample(source, sampler, vout.tex_coords);
    return vec4<f32>(srgb_scene_to_monitor(sampled.rgb), sampled.a);
}