struct gl_PerVertex {
    [[builtin(position)]] gl_Position: vec4<f32>;
};

struct VertexOutput {
    [[location(0)]] member: vec2<f32>;
    [[builtin(position)]] gl_Position: vec4<f32>;
};

var<private> gl_VertexIndex_1: i32;
var<private> o_clip_position: vec2<f32>;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );

fn main_1() {
    let _e13 = gl_VertexIndex_1;
    let _e14 = bitcast<u32>(_e13);
    let _e18 = ((f32((_e14 / 2u)) * 4.0) - 1.0);
    let _e22 = ((f32((_e14 % 2u)) * 4.0) - 1.0);
    o_clip_position = vec2<f32>(_e18, _e22);
    perVertexStruct.gl_Position = vec4<f32>(_e18, _e22, 0.0, 1.0);
    return;
}

[[stage(vertex)]]
fn main([[builtin(vertex_index)]] gl_VertexIndex: u32) -> VertexOutput {
    gl_VertexIndex_1 = i32(gl_VertexIndex);
    main_1();
    let _e6 = o_clip_position;
    let _e7 = perVertexStruct.gl_Position;
    return VertexOutput(_e6, _e7);
}
