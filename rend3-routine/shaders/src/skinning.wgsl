struct Range {
    start: u32;
    end: u32;
};

/// See documentation for the same struct in skinning/mod.rs
struct GpuSkinningInput {
    mesh_range: Range;
    skeleton_range: Range;
    joints_start_idx: u32;
};

struct JointMatrices {
    matrices: array<mat4x4<f32>>;
};

/// The arrays are tightly packed and `vec3` does not have the right stride
struct Vec3 { x: f32; y: f32; z: f32; };
struct Vec3Array { data: array<Vec3>; };

struct JointWeightVec { w0: f32; w1: f32; w2: f32; w3: f32; };
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
var<uniform> input : GpuSkinningInput;

fn to_v(v: Vec3) -> vec3<f32> {
    return vec3<f32>(v.x, v.y, v.z);
}
fn from_v(v: vec3<f32>) -> Vec3 {
    var res : Vec3;
    res.x = v.x; res.y = v.y; res.z = v.z;
    return res;
}


fn get_joint_matrix(joint_idx: u32) -> mat4x4<f32> {
    return joint_matrices.matrices[input.joints_start_idx + joint_idx];
}

[[stage(compute), workgroup_size(64)]]
fn main([[builtin(global_invocation_id)]] global_id: vec3<u32>) {
    let idx = global_id.x;

    let count = input.mesh_range.end - input.mesh_range.start;
    if (idx >= count) {
        return;
    }

    let pos = to_v(positions.data[input.mesh_range.start + idx]);
    let normal = to_v(normals.data[input.mesh_range.start + idx]);
    let tangent = to_v(tangents.data[input.mesh_range.start + idx]);
    let joint_ws = joint_weights.data[input.mesh_range.start + idx];
    let joint_is = joint_indices.data[input.mesh_range.start + idx];

    // NOTE: This should use bitfieldExtract once it is available on dx12
    let joint_i0 = joint_is.indices_0_1 & 0x0000ffffu;
    let joint_i1 = (joint_is.indices_0_1 & 0xffff0000u) >> 16u;
    let joint_i2 = joint_is.indices_2_3 & 0x0000ffffu;
    let joint_i3 = (joint_is.indices_2_3 & 0xffff0000u) >> 16u;
    
    // Compute the skinned position and write to output region of buffer
    let pos4 = vec4<f32>(pos, 1.0);
    var pos_acc = vec3<f32>(0.0, 0.0, 0.0);
    pos_acc = pos_acc + ((pos4 * get_joint_matrix(joint_i0)) * joint_ws.w0).xyz;
    pos_acc = pos_acc + ((pos4 * get_joint_matrix(joint_i1)) * joint_ws.w1).xyz;
    pos_acc = pos_acc + ((pos4 * get_joint_matrix(joint_i2)) * joint_ws.w2).xyz;
    pos_acc = pos_acc + ((pos4 * get_joint_matrix(joint_i3)) * joint_ws.w3).xyz;
    positions.data[input.skeleton_range.start + idx] = from_v(pos_acc);

    // Compute the skinned normal and write to output region of buffer
    let norm4 = vec4<f32>(normal, 0.0);
    var norm_acc = vec3<f32>(0.0, 0.0, 0.0);
    norm_acc = norm_acc + ((norm4 * get_joint_matrix(joint_i0)) * joint_ws.w0).xyz;
    norm_acc = norm_acc + ((norm4 * get_joint_matrix(joint_i1)) * joint_ws.w1).xyz;
    norm_acc = norm_acc + ((norm4 * get_joint_matrix(joint_i2)) * joint_ws.w2).xyz;
    norm_acc = norm_acc + ((norm4 * get_joint_matrix(joint_i3)) * joint_ws.w3).xyz;
    normals.data[input.skeleton_range.start + idx] = from_v(norm_acc);

    // Compute the skinned tangent and write to output region of buffer
    let tang4 = vec4<f32>(tangent, 0.0);
    var tang_acc = vec3<f32>(0.0, 0.0, 0.0);
    tang_acc = tang_acc + ((tang4 * get_joint_matrix(joint_i0)) * joint_ws.w0).xyz;
    tang_acc = tang_acc + ((tang4 * get_joint_matrix(joint_i1)) * joint_ws.w1).xyz;
    tang_acc = tang_acc + ((tang4 * get_joint_matrix(joint_i2)) * joint_ws.w2).xyz;
    tang_acc = tang_acc + ((tang4 * get_joint_matrix(joint_i3)) * joint_ws.w3).xyz;
    tangents.data[input.skeleton_range.start + idx] = from_v(tang_acc);
}