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

struct IntermediateBufferA {
    result_index_a: [[stride(4)]] array<u32>;
};

var<private> gl_GlobalInvocationID_1: vec3<u32>;
[[group(0), binding(1)]]
var<uniform> unnamed: ObjectInputUniforms;
[[group(0), binding(0)]]
var<storage> unnamed_1: ObjectInputDataBuffer;
[[group(0), binding(2)]]
var<storage, read_write> unnamed_2: IntermediateBufferA;

fn main_1() {
    var phi_502_: bool;

    switch(bitcast<i32>(0u)) {
        default: {
            let _e24 = gl_GlobalInvocationID_1[0u];
            let _e27 = unnamed.uniforms.object_count;
            if ((_e24 >= _e27)) {
                break;
            }
            let _e32 = unnamed_1.object_input[_e24].transform;
            let _e34 = unnamed_1.object_input[_e24].bounding_sphere;
            let _e37 = unnamed.uniforms.view;
            let _e38 = (_e37 * _e32);
            let _e54 = (_e38 * vec4<f32>(_e34.x, _e34.y, _e34.z, 1.0));
            let _e56 = (_e34.w * max(max(length(_e38[0].xyz), length(_e38[1].xyz)), length(_e38[2].xyz)));
            let _e63 = unnamed.uniforms.frustum;
            switch(bitcast<i32>(0u)) {
                default: {
                    let _e75 = vec4<f32>(_e54.x, _e54.y, _e54.z, _e56).xyz;
                    let _e76 = -(_e56);
                    if (!(((dot(_e63.left.inner.xyz, _e75) + _e63.left.inner.w) >= _e76))) {
                        phi_502_ = false;
                        break;
                    }
                    if (!(((dot(_e63.right.inner.xyz, _e75) + _e63.right.inner.w) >= _e76))) {
                        phi_502_ = false;
                        break;
                    }
                    if (!(((dot(_e63.top.inner.xyz, _e75) + _e63.top.inner.w) >= _e76))) {
                        phi_502_ = false;
                        break;
                    }
                    if (!(((dot(_e63.bottom.inner.xyz, _e75) + _e63.bottom.inner.w) >= _e76))) {
                        phi_502_ = false;
                        break;
                    }
                    if (!(((dot(_e63.near.inner.xyz, _e75) + _e63.near.inner.w) >= _e76))) {
                        phi_502_ = false;
                        break;
                    }
                    phi_502_ = true;
                    break;
                }
            }
            let _e108 = phi_502_;
            let _e109 = select(0u, 1u, _e108);
            unnamed_2.result_index_a[_e24] = insertBits(insertBits(0u, _e109, bitcast<u32>(31), bitcast<u32>(1)), _e109, bitcast<u32>(0), bitcast<u32>(31));
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
