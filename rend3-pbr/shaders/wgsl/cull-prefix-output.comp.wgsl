struct Plane {
    inner: vec4<f32>;
};

struct Frustum {
    left: Plane;
    right: Plane;
    top: Plane;
    bottom: Plane;
    near: Plane;
};

struct CullingUniforms {
    view: mat4x4<f32>;
    view_proj: mat4x4<f32>;
    frustum: Frustum;
    object_count: u32;
};

[[block]]
struct ObjectInputUniforms {
    uniforms: CullingUniforms;
};

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

struct IndirectCall {
    vertex_count: u32;
    instance_count: u32;
    base_index: u32;
    vertex_offset: i32;
    base_instance: u32;
};

[[block]]
struct IndirectBuffer {
    draw_call_count: u32;
    pad0: u32;
    pad1: u32;
    pad2: u32;
    indirect_call: [[stride(20)]] array<IndirectCall>;
};

[[block]]
struct IntermediateBufferA {
    result_index_a: [[stride(4)]] array<u32>;
};

struct ObjectInputData {
    start_idx: u32;
    count: u32;
    vertex_offset: i32;
    material_idx: u32;
    transform: mat4x4<f32>;
    bounding_sphere: vec4<f32>;
};

[[block]]
struct ObjectInputDataBuffer {
    object_input: [[stride(96)]] array<ObjectInputData>;
};

[[group(0), binding(1)]]
var<uniform> global: ObjectInputUniforms;
[[group(0), binding(4)]]
var<storage, read_write> global1: ObjectOutputDataBuffer;
[[group(0), binding(5)]]
var<storage, read_write> global2: IndirectBuffer;
var<private> gl_GlobalInvocationID1: vec3<u32>;
[[group(0), binding(2)]]
var<storage, read_write> global3: IntermediateBufferA;
[[group(0), binding(0)]]
var<storage> global4: ObjectInputDataBuffer;

fn main1() {
    switch(bitcast<i32>(0u)) {
        default: {
            let e27: u32 = gl_GlobalInvocationID1[0u];
            let e30: u32 = global.uniforms.object_count;
            if ((e27 >= e30)) {
                break;
            }
            let e34: u32 = global3.result_index_a[e27];
            let e37: u32 = extractBits(e34, bitcast<u32>(0), bitcast<u32>(31));
            let e38: u32 = (e37 - 1u);
            if ((bitcast<i32>(e27) == bitcast<i32>((e30 - 1u)))) {
                global2.draw_call_count = e37;
            }
            if (!((bitcast<i32>(extractBits(e34, bitcast<u32>(31), bitcast<u32>(1))) != bitcast<i32>(0u)))) {
                break;
            }
            let e54: u32 = global4.object_input[e27].start_idx;
            let e56: u32 = global4.object_input[e27].count;
            let e58: i32 = global4.object_input[e27].vertex_offset;
            let e60: u32 = global4.object_input[e27].material_idx;
            let e62: mat4x4<f32> = global4.object_input[e27].transform;
            let e65: mat4x4<f32> = global.uniforms.view;
            let e66: mat4x4<f32> = (e65 * e62);
            let e69: mat4x4<f32> = global.uniforms.view_proj;
            let e72: vec3<f32> = e66[0].xyz;
            let e74: vec3<f32> = e66[1].xyz;
            let e76: vec3<f32> = e66[2].xyz;
            global1.object_output[e38].model_view = e66;
            global1.object_output[e38].model_view_proj = (e69 * e62);
            global1.object_output[e38].inv_squared_scale = (vec3<f32>(1.0, 1.0, 1.0) / vec3<f32>((dot(e72, e72) * sign(determinant(mat3x3<f32>(e72, e74, e76)))), dot(e74, e74), dot(e76, e76)));
            global1.object_output[e38].material_idx = e60;
            global2.indirect_call[e38].vertex_count = e56;
            global2.indirect_call[e38].instance_count = 1u;
            global2.indirect_call[e38].base_index = e54;
            global2.indirect_call[e38].vertex_offset = e58;
            global2.indirect_call[e38].base_instance = e38;
            break;
        }
    }
    return;
}

[[stage(compute), workgroup_size(256, 1, 1)]]
fn main([[builtin(global_invocation_id)]] gl_GlobalInvocationID: vec3<u32>) {
    gl_GlobalInvocationID1 = gl_GlobalInvocationID;
    main1();
}
