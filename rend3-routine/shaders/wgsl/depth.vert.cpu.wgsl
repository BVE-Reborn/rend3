struct ObjectOutputData {
    model_view: mat4x4<f32>,
    model_view_proj: mat4x4<f32>,
    material_idx: u32,
    inv_squared_scale: vec3<f32>,
}

struct ObjectOutputDataBuffer {
    object_output: array<ObjectOutputData>,
}

struct gl_PerVertex {
    @builtin(position) @invariant gl_Position: vec4<f32>,
}

struct VertexOutput {
    @location(0) member: vec4<f32>,
    @builtin(position) @invariant gl_Position: vec4<f32>,
    @location(3) @interpolate(flat) member_1: u32,
    @location(2) member_2: vec4<f32>,
    @location(1) member_3: vec2<f32>,
}

var<private> gl_InstanceIndex_1: i32;
@group(1) @binding(0) 
var<storage> unnamed: ObjectOutputDataBuffer;
var<private> i_position_1: vec3<f32>;
var<private> o_position: vec4<f32>;
var<private> perVertexStruct: gl_PerVertex = gl_PerVertex(vec4<f32>(0.0, 0.0, 0.0, 1.0), );
var<private> o_material: u32;
var<private> o_color: vec4<f32>;
var<private> i_color_1: vec4<f32>;
var<private> o_coords0_: vec2<f32>;
var<private> i_coords0_1: vec2<f32>;

fn main_1() {
    let _e18 = gl_InstanceIndex_1;
    let _e23 = unnamed.object_output[bitcast<u32>(_e18)].model_view_proj;
    let _e25 = unnamed.object_output[bitcast<u32>(_e18)].material_idx;
    let _e26 = i_position_1;
    let _e31 = (_e23 * vec4<f32>(_e26.x, _e26.y, _e26.z, 1.0));
    o_position = _e31;
    perVertexStruct.gl_Position = _e31;
    o_material = _e25;
    let _e33 = i_color_1;
    o_color = _e33;
    let _e34 = i_coords0_1;
    o_coords0_ = _e34;
    return;
}

@vertex 
fn main(@builtin(instance_index) gl_InstanceIndex: u32, @location(0) i_position: vec3<f32>, @location(5) i_color: vec4<f32>, @location(3) i_coords0_: vec2<f32>) -> VertexOutput {
    gl_InstanceIndex_1 = i32(gl_InstanceIndex);
    i_position_1 = i_position;
    i_color_1 = i_color;
    i_coords0_1 = i_coords0_;
    main_1();
    let _e15 = o_position;
    let _e16 = perVertexStruct.gl_Position;
    let _e17 = o_material;
    let _e18 = o_color;
    let _e19 = o_coords0_;
    return VertexOutput(_e15, _e16, _e17, _e18, _e19);
}
