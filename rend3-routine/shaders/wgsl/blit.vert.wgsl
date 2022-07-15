struct gl_PerVertex {
    @builtin(position) @invariant l_Position: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) @invariant gl_Position: vec4<f32>,
    @location(0) member: vec2<f32>,
}

var<private> gl_VertexIndex_1: i32;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );
var<private> tex_coords: vec2<f32>;

fn main_1() {
    let _e15 = gl_VertexIndex_1;
    let _e16 = bitcast<u32>(_e15);
    let _e18 = f32((_e16 / 2u));
    let _e21 = f32((_e16 % 2u));
    perVertexStruct.gl_Position = vec4<f32>(fma(_e18, 4.0, -1.0), fma(_e21, 4.0, -1.0), 0.0, 1.0);
    tex_coords = vec2<f32>((_e18 * 2.0), fma(-(_e21), 2.0, 1.0));
    return;
}

@vertex 
fn main(@builtin(vertex_index) gl_VertexIndex: u32) -> VertexOutput {
    gl_VertexIndex_1 = i32(gl_VertexIndex);
    main_1(); 
    let _e6 = perVertexStruct.gl_Position;
    let _e7 = tex_coords;
    return VertexOutput(_e6, _e7);
}
