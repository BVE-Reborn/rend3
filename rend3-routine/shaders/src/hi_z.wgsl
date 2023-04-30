@group(0) @binding(0)
var source: texture_depth_2d;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) resolution: vec2<u32>,
}

@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VertexOutput {
    let resolution = vec2<u32>(textureDimensions(source));
    var output: VertexOutput;
    output.position = vec4<f32>(f32(id / 2u) * 4.0 - 1.0, f32(id % 2u) * 4.0 - 1.0, 0.0, 1.0);
    output.resolution = resolution;
    return output;
}

@fragment
fn fs_main(vout: VertexOutput) -> @builtin(frag_depth) f32 {
    let this_tex_coord = vec2<u32>(vout.position.xy);
    let previous_base_tex_coord = 2u * this_tex_coord;

    let x_count_odd = vout.resolution.x & 1u;
    let y_count_odd = vout.resolution.y & 1u;

    var nearest = 1.0;
    for (var x = 0u; x < 2u + x_count_odd; x += 1u) {
        for (var y = 0u; y < 2u + y_count_odd; y += 1u) {
            nearest = min(nearest, textureLoad(source, previous_base_tex_coord + vec2<u32>(x, y), 0));
        }
    }

    return nearest;
}