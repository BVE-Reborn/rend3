struct Range {
    start: u32;
    end: u32;
};

/// See documentation for the same struct in skinning/mod.rs
struct GpuSkinningInput {
    mesh_range: Range;
    skeleton_range: Range;
    joint_idx: u32;
};

struct JointMatrices {
    matrices: array<mat4x4<f32>>;
};

/// The arrays are tightly packed and `vec3` does not have the right stride
struct Vec3 { x: f32; y: f32; z: f32; };
struct Vec3Array { data: array<Vec3>; };

struct JointWeightVec { w1: f32; w2: f32; w3: f32; w4: f32; };
struct JointWeightVecArray { data: array<JointWeightVec>; };

/// The u16 type does not exist in shaders, so we need to unpack the u16 indices
/// from a pair of u32s
struct JointIndexVec { indices_0_1: u32; indices_2_3: u32; };
struct JointIndexVecArray { data: array<JointIndexVec>; };

[[group(0), binding(0)]]
var<storage, read_write> positions: Vec3Array;

[[group(0), binding(1)]]
var<storage, read_write> normals: Vec3Array;

[[group(0), binding(2)]]
var<storage, read_write> tangents: Vec3Array;

[[group(0), binding(3)]]
var<storage, read_write> joint_indices: JointIndexVecArray;

[[group(0), binding(4)]]
var<storage, read_write> joint_weights: JointWeightVecArray;

[[group(1), binding(0)]]
var<storage> joint_matrices: JointMatrices;

[[group(2), binding(0)]]
var<storage> input : GpuSkinningInput;

fn to_v(v: Vec3) -> vec3<f32> {
    return vec3<f32>(v.x, v.y, v.z);
}
fn from_v(v: vec3<f32>) -> Vec3 {
    var res : Vec3;
    res.x = v.x; res.y = v.y; res.z = v.z;
    return res;
}

[[stage(compute), workgroup_size(64)]]
fn main([[builtin(global_invocation_id)]] global_id: vec3<u32>) {
    // TODO: Transform positions, normals and tangents
}