[[block]]
struct gl_PerVertex {
    [[builtin(position)]] gl_Position: vec4<f32>;
};

struct VertexOutput {
    [[location(0)]] member: vec2<f32>;
    [[builtin(position)]] gl_Position: vec4<f32>;
};

var<private> gl_VertexIndex1: i32;
var<private> o_clip_position: vec2<f32>;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );

fn main1() {
    let e13: i32 = gl_VertexIndex1;
    let e14: u32 = bitcast<u32>(e13);
    let e18: f32 = ((f32((e14 / 2u)) * 4.0) - 1.0);
    let e22: f32 = ((f32((e14 % 2u)) * 4.0) - 1.0);
    o_clip_position = vec2<f32>(e18, e22);
    perVertexStruct.gl_Position = vec4<f32>(e18, e22, 0.0, 1.0);
    return;
}

[[stage(vertex)]]
fn main([[builtin(vertex_index)]] gl_VertexIndex: u32) -> VertexOutput {
    gl_VertexIndex1 = i32(gl_VertexIndex);
    main1();
    let e6: vec2<f32> = o_clip_position;
    let e7: vec4<f32> = perVertexStruct.gl_Position;
    return VertexOutput(e6, e7);
}
