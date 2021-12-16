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

struct PushConstants {
    stride: u32;
};

struct IntermediateBufferB {
    result_index_b: [[stride(4)]] array<u32>;
};

struct IntermediateBufferA {
    result_index_a: [[stride(4)]] array<u32>;
};

var<private> gl_GlobalInvocationID_1: vec3<u32>;
[[group(0), binding(1)]]
var<uniform> unnamed: ObjectInputUniforms;
var<push_constant> unnamed_1: PushConstants;
[[group(0), binding(3)]]
var<storage, read_write> unnamed_2: IntermediateBufferB;
[[group(0), binding(2)]]
var<storage, read_write> unnamed_3: IntermediateBufferA;

fn main_1() {
    switch(bitcast<i32>(0u)) {
        default: {
            let _e18 = gl_GlobalInvocationID_1[0u];
            let _e21 = unnamed.uniforms.object_count;
            if ((_e18 >= _e21)) {
                break;
            }
            let _e24 = unnamed_1.stride;
            if ((_e18 < _e24)) {
                let _e28 = unnamed_3.result_index_a[_e18];
                unnamed_2.result_index_b[_e18] = _e28;
                break;
            }
            let _e34 = unnamed_3.result_index_a[_e18];
            let _e37 = unnamed_3.result_index_a[(_e18 - _e24)];
            unnamed_2.result_index_b[_e18] = insertBits(_e34, (extractBits(_e34, bitcast<u32>(0), bitcast<u32>(31)) + extractBits(_e37, bitcast<u32>(0), bitcast<u32>(31))), bitcast<u32>(0), bitcast<u32>(31));
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
