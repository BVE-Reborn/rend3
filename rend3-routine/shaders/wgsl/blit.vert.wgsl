struct gl_PerVertex {
    [[builtin(position)]] gl_Position: vec4<f32>;
};

struct VertexOutput {
    [[builtin(position)]] gl_Position: vec4<f32>;
    [[location(0)]] member: vec2<f32>;
};

var<private> gl_VertexIndex_1: i32;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );
var<private> tex_coords: vec2<f32>;

fn main_1() {
    let _e14 = gl_VertexIndex_1;
    let _e15 = bitcast<u32>(_e14);
    let _e17 = f32((_e15 / 2u));
    let _e21 = f32((_e15 % 2u));
    perVertexStruct.gl_Position = vec4<f32>(((_e17 * 4.0) - 1.0), ((_e21 * 4.0) - 1.0), 0.0, 1.0);
    tex_coords = vec2<f32>((_e17 * 2.0), (1.0 - (_e21 * 2.0)));
    return;
}

[[stage(vertex)]]
fn main([[builtin(vertex_index)]] gl_VertexIndex: u32) -> VertexOutput {
    gl_VertexIndex_1 = i32(gl_VertexIndex);
    main_1();
    let _e6 = perVertexStruct.gl_Position;
    let _e7 = tex_coords;
    return VertexOutput(_e6, _e7);
}
