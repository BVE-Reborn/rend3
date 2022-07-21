{{include "rend3-routine/math/frustum.wgsl"}}
{{include "rend3-routine/math/matrix.wgsl"}}
{{include "rend3-routine/math/sphere.wgsl"}}
{{include "rend3-routine/structures.wgsl"}}

struct CullingUniforms {
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    frustum: Frustum,
    object_count: u32,
}

struct IndirectBuffer {
    count: atomic<u32>,
    pad0: u32,
    pad1: u32,
    pad2: u32,
    calls: array<IndirectCall>,
}

@group(0) @binding(0)
var<storage, read> object_input: array<ObjectInputData>;
@group(0) @binding(1)
var<uniform> uniforms: CullingUniforms;
@group(0) @binding(2)
var<storage, read_write> object_output: array<ObjectOutputData>;
@group(0) @binding(3)
var<storage, read_write> draw_call_output: IndirectBuffer;
@group(0) @binding(4)
var<storage, read_write> result_index_a: array<u32>;
@group(0) @binding(5)
var<storage, read_write> result_index_b: array<u32>;

var<push_constant> stride: u32;

fn write_output(in_data: ObjectInputData, index: u32) {
    var out_data: ObjectOutputData;

    out_data.model_view = uniforms.view * in_data.transform;
    out_data.model_view_proj = uniforms.view_proj * in_data.transform;
    out_data.inv_scale_sq = mat3_inv_scale_squared(
        mat3x3<f32>(
            out_data.model_view[0].xyz,
            out_data.model_view[1].xyz,
            out_data.model_view[2].xyz
        )
    );
    out_data.material_idx = in_data.material_idx;

    object_output[index] = out_data;

    var call: IndirectCall;
    call.vertex_count = in_data.count;
    call.instance_count = 1u;
    call.base_index = in_data.start_idx;
    call.vertex_offset = in_data.vertex_offset;
    call.base_instance = index;
    draw_call_output.calls[index] = call;
}

@compute @workgroup_size(256)
fn atomic_main(@builtin(global_invocation_id) input_idx_vec: vec3<u32>) {
    let input_idx = input_idx_vec.x;
    if (input_idx >= uniforms.object_count) {
        return;
    }

    let in_data = object_input[input_idx];

    let model_view = uniforms.view * in_data.transform;
    let mesh_sphere = sphere_transform_by_mat4(in_data.bounding_sphere, model_view);

    let visible = frustum_contains_sphere(uniforms.frustum, mesh_sphere);

    if (!visible) {
        return;
    }

    let output_idx = atomicAdd(&draw_call_output.count, 1u);

    write_output(in_data, output_idx);
}

@compute @workgroup_size(256)
fn prefix_begin(@builtin(global_invocation_id) input_idx_vec: vec3<u32>) {
    let input_idx = input_idx_vec.x;
    if (input_idx >= uniforms.object_count) {
        return;
    }

    let in_data = object_input[input_idx];

    let model_view = uniforms.view * in_data.transform;
    let mesh_sphere = sphere_transform_by_mat4(in_data.bounding_sphere, model_view);

    let visible = frustum_contains_sphere(uniforms.frustum, mesh_sphere);

    let result_index = u32(visible) << 31u;
    let result_index2 = result_index | u32(visible);

    result_index_a[input_idx] = result_index2;
}

@compute @workgroup_size(256)
fn prefix_intermediate(@builtin(global_invocation_id) input_idx: vec3<u32>) {
    let my_idx = input_idx.x;
    
    if (my_idx >= uniforms.object_count) {
        return;
    }

    if (my_idx < stride) {
        result_index_b[my_idx] = result_index_a[my_idx];
        return;
    }

    let other_idx = my_idx - stride;

    let in_data = object_input[my_idx];

    let me = result_index_a[my_idx];
    let other = result_index_a[other_idx];
    let me_high_bit = me & 0x80000000u;

    let low_bits_me = me & 0x7FFFFFFFu;
    let low_bits_other = other & 0x7FFFFFFFu;

    let result = me_high_bit | (low_bits_me + low_bits_other);

    result_index_b[my_idx] = result;
}

@compute @workgroup_size(256)
fn prefix_end(@builtin(global_invocation_id) input_idx_vec: vec3<u32>) {
    let input_idx = input_idx_vec.x;
    if (input_idx >= uniforms.object_count) {
        return;
    }
    
    let result = result_index_a[input_idx];
    let objects_before = result & 0x7FFFFFFFu;
    let enabled = bool(result & 0x80000000u);

    let output_idx = objects_before - 1u;

    if (input_idx == uniforms.object_count - 1u) {
        atomicStore(&draw_call_output.count, 1u);
    }

    if (!enabled) {
        return;
    }

    write_output(object_input[input_idx], output_idx);
}
