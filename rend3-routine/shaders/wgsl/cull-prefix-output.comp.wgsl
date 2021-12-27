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

struct ObjectInputUniforms {
    uniforms: CullingUniforms;
};

struct ObjectOutputData {
    model_view: mat4x4<f32>;
    model_view_proj: mat4x4<f32>;
    material_idx: u32;
    inv_squared_scale: vec3<f32>;
};

struct ObjectOutputDataBuffer {
    object_output: [[stride(160)]] array<ObjectOutputData>;
};

struct IndirectCall {
    vertex_count: u32;
    instance_count: u32;
    base_index: u32;
    vertex_offset: i32;
    base_instance: u32;
};

struct IndirectBuffer {
    draw_call_count: u32;
    pad0_: u32;
    pad1_: u32;
    pad2_: u32;
    indirect_call: [[stride(20)]] array<IndirectCall>;
};

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

struct ObjectInputDataBuffer {
    object_input: [[stride(96)]] array<ObjectInputData>;
};

[[group(0), binding(1)]]
var<uniform> unnamed: ObjectInputUniforms;
[[group(0), binding(4)]]
var<storage, read_write> unnamed_1: ObjectOutputDataBuffer;
[[group(0), binding(5)]]
var<storage, read_write> unnamed_2: IndirectBuffer;
var<private> gl_GlobalInvocationID_1: vec3<u32>;
[[group(0), binding(2)]]
var<storage, read_write> unnamed_3: IntermediateBufferA;
[[group(0), binding(0)]]
var<storage> unnamed_4: ObjectInputDataBuffer;

fn main_1() {
    switch(bitcast<i32>(0u)) {
        default: {
            let _e27 = gl_GlobalInvocationID_1[0u];
            let _e30 = unnamed.uniforms.object_count;
            if ((_e27 >= _e30)) {
                break;
            }
            let _e34 = unnamed_3.result_index_a[_e27];
            let _e37 = extractBits(_e34, bitcast<u32>(0), bitcast<u32>(31));
            let _e38 = (_e37 - 1u);
            if ((_e27 == (_e30 - 1u))) {
                unnamed_2.draw_call_count = _e37;
            }
            if (!((extractBits(_e34, bitcast<u32>(31), bitcast<u32>(1)) != 0u))) {
                break;
            }
            let _e50 = unnamed_4.object_input[_e27].start_idx;
            let _e52 = unnamed_4.object_input[_e27].count;
            let _e54 = unnamed_4.object_input[_e27].vertex_offset;
            let _e56 = unnamed_4.object_input[_e27].material_idx;
            let _e58 = unnamed_4.object_input[_e27].transform;
            let _e61 = unnamed.uniforms.view;
            let _e62 = (_e61 * _e58);
            let _e65 = unnamed.uniforms.view_proj;
            let _e68 = _e62[0].xyz;
            let _e70 = _e62[1].xyz;
            let _e72 = _e62[2].xyz;
            unnamed_1.object_output[_e38].model_view = _e62;
            unnamed_1.object_output[_e38].model_view_proj = (_e65 * _e58);
            unnamed_1.object_output[_e38].material_idx = _e56;
            unnamed_1.object_output[_e38].inv_squared_scale = (vec3<f32>(1.0, 1.0, 1.0) / vec3<f32>(dot(_e68, _e68), dot(_e70, _e70), dot(_e72, _e72)));
            unnamed_2.indirect_call[_e38].vertex_count = _e52;
            unnamed_2.indirect_call[_e38].instance_count = 1u;
            unnamed_2.indirect_call[_e38].base_index = _e50;
            unnamed_2.indirect_call[_e38].vertex_offset = _e54;
            unnamed_2.indirect_call[_e38].base_instance = _e38;
            break;
        }
    }
    return;
}

[[stage(compute), workgroup_size(256, 1, 1)]]
fn main([[builtin(global_invocation_id)]] gl_GlobalInvocationID: vec3<u32>) {
    gl_GlobalInvocationID_1 = gl_GlobalInvocationID;
    main_1();
}
