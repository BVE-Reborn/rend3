{{include "rend3-routine/math/matrix.wgsl"}}
{{include "rend3-routine/structures.wgsl"}}
{{include "rend3-routine/structures_object.wgsl"}}

struct SkinningInput {
    /// Byte offset into vertex buffer of position attribute of unskinned mesh.
    base_position_offset: u32,
    /// Byte offset into vertex buffer of normal attribute of unskinned mesh.
    base_normal_offset: u32,
    /// Byte offset into vertex buffer of tangent attribute of unskinned mesh.
    base_tangent_offset: u32,
    /// Byte offset into vertex buffer of joint indices of mesh.
    joint_indices_offset: u32,
    /// Byte offset into vertex buffer of joint weights of mesh.
    joint_weight_offset: u32,
    /// Byte offset into vertex buffer of position attribute of skinned mesh.
    updated_position_offset: u32,
    /// Byte offset into vertex buffer of normal attribute of skinned mesh.
    updated_normal_offset: u32,
    /// Byte offset into vertex buffer of tangent attribute of skinned mesh.
    updated_tangent_offset: u32,

    /// Index into the matrix buffer that joint_indices is relative to.
    joint_matrix_base_offset: u32,
    /// Count of vertices in this mesh.
    vertex_count: u32,
}

@group(0) @binding(0)
var<storage, read_write> vertex_buffer: array<u32>;
@group(0) @binding(1)
var<storage> input: SkinningInput;
@group(0) @binding(2)
var<storage> joint_matrices: array<mat4x4<f32>>;

{{include "rend3/vertex_attributes.wgsl"}}
{{include "rend3/vertex_attributes_store.wgsl"}}

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let idx = global_id.x;

    if (idx >= input.vertex_count) {
        return;
    }

    let joint_indices = extract_attribute_vec4_u16(input.joint_indices_offset, idx);
    let joint_weights = extract_attribute_vec4_f32(input.joint_weight_offset, idx);

    // Compute the skinned position
    var pos_acc = vec3<f32>(0.0, 0.0, 0.0);
    var norm_acc = vec3<f32>(0.0, 0.0, 0.0);
    var tang_acc = vec3<f32>(0.0, 0.0, 0.0);

    let pos = extract_attribute_vec3_f32(input.base_position_offset, idx);
    let normal = extract_attribute_vec3_f32(input.base_normal_offset, idx);
    let tangent = extract_attribute_vec3_f32(input.base_tangent_offset, idx);
    
    for (var i = 0; i < 4; i++) {
        let weight = joint_weights[i];

        if (weight > 0.0) {
            let joint_index = joint_indices[i];
            let joint_matrix = joint_matrices[input.joint_matrix_base_offset + joint_index];
            let joint_matrix3 = mat3x3<f32>(joint_matrix[0].xyz, joint_matrix[1].xyz, joint_matrix[2].xyz);
            pos_acc += (joint_matrix * vec4<f32>(pos, 1.0)).xyz * weight;
            
            let inv_scale_sq = mat3_inv_scale_squared(joint_matrix3);
            norm_acc += (joint_matrix3 * (inv_scale_sq * normal)) * weight;
            tang_acc += (joint_matrix3 * (inv_scale_sq * tangent)) * weight;
        }
    }

    pos_acc = normalize(pos_acc);
    norm_acc = normalize(norm_acc);
    tang_acc = normalize(tang_acc);
    
    // Write to output region of buffer
    store_attribute_vec3_f32(input.updated_position_offset, idx, pos_acc);
    store_attribute_vec3_f32(input.updated_normal_offset, idx, norm_acc);
    store_attribute_vec3_f32(input.updated_tangent_offset, idx, tang_acc);
}