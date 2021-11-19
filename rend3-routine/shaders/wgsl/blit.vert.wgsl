[[block]]
struct gl_PerVertex {
    [[builtin(position)]] gl_Position: vec4<f32>;
};

struct VertexOutput {
    [[builtin(position)]] gl_Position: vec4<f32>;
    [[location(0)]] member: vec2<f32>;
};

var<private> gl_VertexIndex1: i32;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );
var<private> tex_coords: vec2<f32>;

fn main1() {
    let e14: i32 = gl_VertexIndex1;
    let e15: u32 = bitcast<u32>(e14);
    let e17: f32 = f32((e15 / 2u));
    let e21: f32 = f32((e15 % 2u));
    perVertexStruct.gl_Position = vec4<f32>(((e17 * 4.0) - 1.0), ((e21 * 4.0) - 1.0), 0.0, 1.0);
    tex_coords = vec2<f32>((e17 * 2.0), (1.0 - (e21 * 2.0)));
    return;
}

[[stage(vertex)]]
fn main([[builtin(vertex_index)]] gl_VertexIndex: u32) -> VertexOutput {
    gl_VertexIndex1 = i32(gl_VertexIndex);
    main1();
    let e6: vec4<f32> = perVertexStruct.gl_Position;
    let e7: vec2<f32> = tex_coords;
    return VertexOutput(e6, e7);
}
