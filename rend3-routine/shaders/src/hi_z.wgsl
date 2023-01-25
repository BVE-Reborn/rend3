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
    output.tex_coords = vec2<f32>(f32(id / 2u) * 2.0, 1.0 - (f32(id % 2u) * 2.0)) * resolution - 0.5;
    return output;
}

@fragment
fn fs_main(vout: VertexOutput) -> @builtin(frag_depth) f32 {
    let down = floor(vout.tex_coords);
    let up = ceil(vout.tex_coords);

    let top_left = vec2<f32>(down.x, down.y);
    let top_right = vec2<f32>(up.x, down.y);
    let bottom_left = vec2<f32>(down.x, up.y);
    let bottom_right = vec2<f32>(up.x, up.y);

    let top_left_value = textureLoad(source, vec2<u32>(top_left), 0);
    let top_right_value = textureLoad(source, vec2<u32>(top_right), 0);
    let bottom_left_value = textureLoad(source, vec2<u32>(bottom_left), 0);
    let bottom_right_value = textureLoad(source, vec2<u32>(bottom_right), 0);

    let min = min(min(top_left_value, top_right_value), min(bottom_left_value, bottom_right_value));

    return min;
}