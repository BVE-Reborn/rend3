struct ObjectOutputData {
    model_view: mat4x4<f32>;
    model_view_proj: mat4x4<f32>;
    material_idx: u32;
    inv_squared_scale: vec3<f32>;
};

struct ObjectOutputDataBuffer {
    object_output: [[stride(160)]] array<ObjectOutputData>;
};

struct CPUMaterialData {
    uv_transform0_: mat3x3<f32>;
    uv_transform1_: mat3x3<f32>;
    albedo: vec4<f32>;
    emissive: vec3<f32>;
    roughness: f32;
    metallic: f32;
    reflectance: f32;
    clear_coat: f32;
    clear_coat_roughness: f32;
    anisotropy: f32;
    ambient_occlusion: f32;
    alpha_cutout: f32;
    material_flags: u32;
    texture_enable: u32;
};

struct TextureData {
    material: CPUMaterialData;
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
[[group(2), binding(10)]]
var<uniform> unnamed_1: TextureData;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );
var<private> i_material_1: u32;

fn main_1() {
    let _e32 = gl_InstanceIndex_1;
    let _e37 = unnamed.object_output[bitcast<u32>(_e32)].model_view;
    let _e39 = unnamed.object_output[bitcast<u32>(_e32)].material_idx;
    let _e41 = unnamed.object_output[bitcast<u32>(_e32)].inv_squared_scale;
    o_material = _e39;
    let _e42 = i_position_1;
    o_view_position = (_e37 * vec4<f32>(_e42.x, _e42.y, _e42.z, 1.0));
    let _e54 = mat3x3<f32>(_e37[0].xyz, _e37[1].xyz, _e37[2].xyz);
    let _e55 = i_normal_1;
    o_normal = (_e54 * (_e41 * _e55));
    let _e58 = i_tangent_1;
    o_tangent = (_e54 * (_e41 * _e58));
    let _e61 = i_color_1;
    o_color = _e61;
    let _e62 = i_coords0_1;
    o_coords0_ = _e62;
    let _e63 = i_coords1_1;
    o_coords1_ = _e63;
    let _e66 = unnamed_1.material.uv_transform1_;
    let _e70 = (_e66 * vec3<f32>(_e63.x, _e63.y, 1.0));
    let _e75 = ((vec2<f32>(_e70.x, _e70.y) * 2.0) - vec2<f32>(1.0, 1.0));
    perVertexStruct.gl_Position = vec4<f32>(_e75.x, _e75.y, 0.0, 1.0);
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
