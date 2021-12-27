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
    [[location(6)]] member: u32;
    [[location(0)]] member_1: vec4<f32>;
    [[location(1)]] member_2: vec3<f32>;
    [[location(2)]] member_3: vec3<f32>;
    [[location(5)]] member_4: vec4<f32>;
    [[location(3)]] member_5: vec2<f32>;
    [[location(4)]] member_6: vec2<f32>;
    [[builtin(position)]] gl_Position: vec4<f32>;
};

var<private> gl_InstanceIndex_1: i32;
[[group(1), binding(0)]]
var<storage> unnamed: ObjectOutputDataBuffer;
var<private> o_material: u32;
var<private> o_view_position: vec4<f32>;
var<private> i_position_1: vec3<f32>;
var<private> o_normal: vec3<f32>;
var<private> i_normal_1: vec3<f32>;
var<private> o_tangent: vec3<f32>;
var<private> i_tangent_1: vec3<f32>;
var<private> o_color: vec4<f32>;
var<private> i_color_1: vec4<f32>;
var<private> o_coords0_: vec2<f32>;
var<private> i_coords0_1: vec2<f32>;
var<private> o_coords1_: vec2<f32>;
var<private> i_coords1_1: vec2<f32>;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );
var<private> i_material_1: u32;

fn main_1() {
    let _e24 = gl_InstanceIndex_1;
    let _e28 = unnamed.object_output[bitcast<u32>(_e24)];
    o_material = _e28.material_idx;
    let _e33 = i_position_1;
    let _e37 = vec4<f32>(_e33.x, _e33.y, _e33.z, 1.0);
    o_view_position = (_e28.model_view * _e37);
    let _e45 = mat3x3<f32>(_e28.model_view[0].xyz, _e28.model_view[1].xyz, _e28.model_view[2].xyz);
    let _e46 = i_normal_1;
    o_normal = (_e45 * (_e28.inv_squared_scale * _e46));
    let _e49 = i_tangent_1;
    o_tangent = (_e45 * (_e28.inv_squared_scale * _e49));
    let _e52 = i_color_1;
    o_color = _e52;
    let _e53 = i_coords0_1;
    o_coords0_ = _e53;
    let _e54 = i_coords1_1;
    o_coords1_ = _e54;
    perVertexStruct.gl_Position = (_e28.model_view_proj * _e37);
    return;
}

[[stage(vertex)]]
fn main([[builtin(instance_index)]] gl_InstanceIndex: u32, [[location(0)]] i_position: vec3<f32>, [[location(1)]] i_normal: vec3<f32>, [[location(2)]] i_tangent: vec3<f32>, [[location(5)]] i_color: vec4<f32>, [[location(3)]] i_coords0_: vec2<f32>, [[location(4)]] i_coords1_: vec2<f32>, [[location(6)]] i_material: u32) -> VertexOutput {
    gl_InstanceIndex_1 = i32(gl_InstanceIndex);
    i_position_1 = i_position;
    i_normal_1 = i_normal;
    i_tangent_1 = i_tangent;
    i_color_1 = i_color;
    i_coords0_1 = i_coords0_;
    i_coords1_1 = i_coords1_;
    i_material_1 = i_material;
    main_1();
    let _e26 = o_material;
    let _e27 = o_view_position;
    let _e28 = o_normal;
    let _e29 = o_tangent;
    let _e30 = o_color;
    let _e31 = o_coords0_;
    let _e32 = o_coords1_;
    let _e33 = perVertexStruct.gl_Position;
    return VertexOutput(_e26, _e27, _e28, _e29, _e30, _e31, _e32, _e33);
}
