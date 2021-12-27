struct ObjectOutputData {
    model_view: mat4x4<f32>;
    model_view_proj: mat4x4<f32>;
    material_idx: u32;
    inv_squared_scale: vec3<f32>;
};

struct ObjectOutputDataBuffer {
    object_output: [[stride(160)]] array<ObjectOutputData>;
};

struct gl_PerVertex {
    [[builtin(position)]] gl_Position: vec4<f32>;
};

struct VertexOutput {
    [[location(0)]] member: vec4<f32>;
    [[builtin(position)]] gl_Position: vec4<f32>;
    [[location(3)]] member_1: u32;
    [[location(2)]] member_2: vec4<f32>;
    [[location(1)]] member_3: vec2<f32>;
};

var<private> gl_InstanceIndex_1: i32;
[[group(1), binding(0)]]
var<storage> unnamed: ObjectOutputDataBuffer;
var<private> i_position_1: vec3<f32>;
var<private> o_position: vec4<f32>;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );
var<private> o_material: u32;
var<private> o_color: vec4<f32>;
var<private> i_color_1: vec4<f32>;
var<private> o_coords0_: vec2<f32>;
var<private> i_coords0_1: vec2<f32>;
var<private> i_normal_1: vec3<f32>;
var<private> i_tangent_1: vec3<f32>;
var<private> i_coords1_1: vec2<f32>;
var<private> i_material_1: u32;

fn main_1() {
    let _e22 = gl_InstanceIndex_1;
    let _e27 = unnamed.object_output[bitcast<u32>(_e22)].model_view_proj;
    let _e29 = unnamed.object_output[bitcast<u32>(_e22)].material_idx;
    let _e30 = i_position_1;
    let _e35 = (_e27 * vec4<f32>(_e30.x, _e30.y, _e30.z, 1.0));
    o_position = _e35;
    perVertexStruct.gl_Position = _e35;
    o_material = _e29;
    let _e37 = i_color_1;
    o_color = _e37;
    let _e38 = i_coords0_1;
    o_coords0_ = _e38;
    return;
}

[[stage(vertex)]]
fn main([[builtin(instance_index)]] gl_InstanceIndex: u32, [[location(0)]] i_position: vec3<f32>, [[location(5)]] i_color: vec4<f32>, [[location(3)]] i_coords0_: vec2<f32>, [[location(1)]] i_normal: vec3<f32>, [[location(2)]] i_tangent: vec3<f32>, [[location(4)]] i_coords1_: vec2<f32>, [[location(6)]] i_material: u32) -> VertexOutput {
    gl_InstanceIndex_1 = i32(gl_InstanceIndex);
    i_position_1 = i_position;
    i_color_1 = i_color;
    i_coords0_1 = i_coords0_;
    i_normal_1 = i_normal;
    i_tangent_1 = i_tangent;
    i_coords1_1 = i_coords1_;
    i_material_1 = i_material;
    main_1();
    let _e23 = o_position;
    let _e24 = perVertexStruct.gl_Position;
    let _e25 = o_material;
    let _e26 = o_color;
    let _e27 = o_coords0_;
    return VertexOutput(_e23, _e24, _e25, _e26, _e27);
}
