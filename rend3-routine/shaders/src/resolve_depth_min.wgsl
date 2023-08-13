@group(0) @binding(0)
var source: texture_depth_multisampled_2d;

const SAMPLES: i32 = {{SAMPLES}};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> VertexOutput {
    let resolution = vec2<u32>(textureDimensions(source));
    var output: VertexOutput;
    output.position = vec4<f32>(f32(id / 2u) * 4.0 - 1.0, f32(id % 2u) * 4.0 - 1.0, 0.0, 1.0);
    return output;
}

@fragment
fn fs_main(vout: VertexOutput) -> @builtin(frag_depth) f32 {
    var nearest: f32 = 1.0;

    for (var sample = 0; sample < SAMPLES; sample += 1) {
        nearest = min(nearest, textureLoad(source, vec2u(vout.position.xy), sample));
    }

    return nearest;
}
