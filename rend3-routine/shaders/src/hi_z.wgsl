@group(0) @binding(0)
var source: texture_depth_2d;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VertexOutput {
    let resolution = vec2<f32>(textureDimensions(source));
    var output: VertexOutput;
    output.position = vec4<f32>(f32(id / 2u) * 4.0 - 1.0, f32(id % 2u) * 4.0 - 1.0, 0.0, 1.0);
    output.tex_coords = vec2<f32>(f32(id / 2u) * 2.0, 1.0 - (f32(id % 2u) * 2.0)) * resolution;
    return output;
}

@fragment
fn fs_main(vout: VertexOutput) -> @builtin(frag_depth) f32 {
    let dir = vec2<f32>(dpdx(vout.tex_coords.x), dpdy(vout.tex_coords.y)) * 0.5;

    let down = floor(vout.tex_coords - dir);
    let up = ceil(vout.tex_coords + dir);

    var nearest = 1.0;
    for (var x = down.x; x < up.x; x += 1.0) {
        for (var y = down.y; y < up.y; y += 1.0) {
            nearest = min(nearest, textureLoad(source, vec2<u32>(u32(x), u32(y)), 0));
        }
    }

    return nearest;
}