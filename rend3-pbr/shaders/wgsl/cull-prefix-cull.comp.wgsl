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

[[block]]
struct IntermediateBufferA {
    result_index_a: [[stride(4)]] array<u32>;
};

var<private> gl_GlobalInvocationID1: vec3<u32>;
[[group(0), binding(1)]]
var<uniform> global: ObjectInputUniforms;
[[group(0), binding(0)]]
var<storage> global1: ObjectInputDataBuffer;
[[group(0), binding(2)]]
var<storage, read_write> global2: IntermediateBufferA;

fn main1() {
    var phi_502: bool;

    switch(bitcast<i32>(0u)) {
        default: {
            let e24: u32 = gl_GlobalInvocationID1[0u];
            let e27: u32 = global.uniforms.object_count;
            if ((e24 >= e27)) {
                break;
            }
            let e32: mat4x4<f32> = global1.object_input[e24].transform;
            let e34: vec4<f32> = global1.object_input[e24].bounding_sphere;
            let e37: mat4x4<f32> = global.uniforms.view;
            let e38: mat4x4<f32> = (e37 * e32);
            let e54: vec4<f32> = (e38 * vec4<f32>(e34.x, e34.y, e34.z, 1.0));
            let e56: f32 = (e34.w * max(max(length(e38[0].xyz), length(e38[1].xyz)), length(e38[2].xyz)));
            let e63: Frustum = global.uniforms.frustum;
            switch(bitcast<i32>(0u)) {
                default: {
                    let e75: vec3<f32> = vec4<f32>(e54.x, e54.y, e54.z, e56).xyz;
                    let e76: f32 = -(e56);
                    if (!(((dot(e63.left.inner.xyz, e75) + e63.left.inner.w) >= e76))) {
                        phi_502 = false;
                        break;
                    }
                    if (!(((dot(e63.right.inner.xyz, e75) + e63.right.inner.w) >= e76))) {
                        phi_502 = false;
                        break;
                    }
                    if (!(((dot(e63.top.inner.xyz, e75) + e63.top.inner.w) >= e76))) {
                        phi_502 = false;
                        break;
                    }
                    if (!(((dot(e63.bottom.inner.xyz, e75) + e63.bottom.inner.w) >= e76))) {
                        phi_502 = false;
                        break;
                    }
                    if (!(((dot(e63.near.inner.xyz, e75) + e63.near.inner.w) >= e76))) {
                        phi_502 = false;
                        break;
                    }
                    phi_502 = true;
                    break;
                }
            }
            let e108: bool = phi_502;
            let e109: u32 = select(0u, 1u, e108);
            global2.result_index_a[e24] = insertBits(insertBits(0u, e109, bitcast<u32>(31), bitcast<u32>(1)), e109, bitcast<u32>(0), bitcast<u32>(31));
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
