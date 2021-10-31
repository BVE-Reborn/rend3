struct ObjectOutputData {
    model_view: mat4x4<f32>;
    model_view_proj: mat4x4<f32>;
    inv_squared_scale: vec3<f32>;
    material_idx: u32;
};

[[block]]
struct ObjectOutputDataBuffer {
    object_output: [[stride(144)]] array<ObjectOutputData>;
};

struct CPUMaterialData {
    uv_transform0: mat3x3<f32>;
    uv_transform1: mat3x3<f32>;
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

[[block]]
struct TextureData {
    material: CPUMaterialData;
};

[[block]]
struct gl_PerVertex {
    [[builtin(position)]] gl_Position: vec4<f32>;
};

struct VertexOutput {
    [[location(6)]] member: u32;
    [[location(0)]] member1: vec4<f32>;
    [[location(1)]] member2: vec3<f32>;
    [[location(2)]] member3: vec3<f32>;
    [[location(5)]] member4: vec4<f32>;
    [[location(3)]] member5: vec2<f32>;
    [[location(4)]] member6: vec2<f32>;
    [[builtin(position)]] gl_Position: vec4<f32>;
};

var<private> gl_InstanceIndex1: i32;
[[group(0), binding(3)]]
var<storage> global: ObjectOutputDataBuffer;
var<private> o_material: u32;
var<private> o_view_position: vec4<f32>;
var<private> i_position1: vec3<f32>;
var<private> o_normal: vec3<f32>;
var<private> i_normal1: vec3<f32>;
var<private> o_tangent: vec3<f32>;
var<private> i_tangent1: vec3<f32>;
var<private> o_color: vec4<f32>;
var<private> i_color1: vec4<f32>;
var<private> o_coords0: vec2<f32>;
var<private> i_coords0_1: vec2<f32>;
var<private> o_coords1: vec2<f32>;
var<private> i_coords1_1: vec2<f32>;
[[group(1), binding(10)]]
var<uniform> global1: TextureData;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );
var<private> i_material1: u32;

fn main1() {
    let e32: i32 = gl_InstanceIndex1;
    let e37: mat4x4<f32> = global.object_output[bitcast<u32>(e32)].model_view;
    let e39: vec3<f32> = global.object_output[bitcast<u32>(e32)].inv_squared_scale;
    let e41: u32 = global.object_output[bitcast<u32>(e32)].material_idx;
    o_material = e41;
    let e42: vec3<f32> = i_position1;
    o_view_position = (e37 * vec4<f32>(e42.x, e42.y, e42.z, 1.0));
    let e48: vec3<f32> = i_normal1;
    o_normal = (e39 * e48);
    let e50: vec3<f32> = i_tangent1;
    o_tangent = (e39 * e50);
    let e52: vec4<f32> = i_color1;
    o_color = e52;
    let e53: vec2<f32> = i_coords0_1;
    o_coords0 = e53;
    let e54: vec2<f32> = i_coords1_1;
    o_coords1 = e54;
    let e57: mat3x3<f32> = global1.material.uv_transform1;
    let e61: vec3<f32> = (e57 * vec3<f32>(e54.x, e54.y, 1.0));
    let e66: vec2<f32> = ((vec2<f32>(e61.x, e61.y) * 2.0) - vec2<f32>(1.0, 1.0));
    perVertexStruct.gl_Position = vec4<f32>(e66.x, e66.y, 0.0, 1.0);
    return;
}

[[stage(vertex)]]
fn main([[builtin(instance_index)]] gl_InstanceIndex: u32, [[location(0)]] i_position: vec3<f32>, [[location(1)]] i_normal: vec3<f32>, [[location(2)]] i_tangent: vec3<f32>, [[location(5)]] i_color: vec4<f32>, [[location(3)]] i_coords0: vec2<f32>, [[location(4)]] i_coords1: vec2<f32>, [[location(6)]] i_material: u32) -> VertexOutput {
    gl_InstanceIndex1 = i32(gl_InstanceIndex);
    i_position1 = i_position;
    i_normal1 = i_normal;
    i_tangent1 = i_tangent;
    i_color1 = i_color;
    i_coords0_1 = i_coords0;
    i_coords1_1 = i_coords1;
    i_material1 = i_material;
    main1();
    let e26: u32 = o_material;
    let e27: vec4<f32> = o_view_position;
    let e28: vec3<f32> = o_normal;
    let e29: vec3<f32> = o_tangent;
    let e30: vec4<f32> = o_color;
    let e31: vec2<f32> = o_coords0;
    let e32: vec2<f32> = o_coords1;
    let e33: vec4<f32> = perVertexStruct.gl_Position;
    return VertexOutput(e26, e27, e28, e29, e30, e31, e32, e33);
}
