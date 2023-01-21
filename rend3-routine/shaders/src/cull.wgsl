{{include "rend3-routine/structures.wgsl"}}
{{include "rend3-routine/structures_object.wgsl"}}

@group(0) @binding(0)
var<storage> vertex_buffer: array<u32>;
@group(0) @binding(1)
var<storage> object_buffer: array<Object>;
@group(0) @binding(2)
var<storage> culling_job: BatchData;
@group(0) @binding(3)
var<storage, read_write> primary_draw_calls: array<IndirectCall>;
@group(0) @binding(4)
var<storage, read_write> secondary_draw_calls: array<IndirectCall>;
@group(0) @binding(5)
var<storage, read_write> primary_output: array<u32>;
@group(0) @binding(6)
var<storage, read_write> secondary_output: array<u32>;
@group(0) @binding(7)
var<storage> previous_culling_results: array<u32>;
@group(0) @binding(8)
var<storage, read_write> current_culling_results: array<u32>;
@group(0) @binding(9)
var<storage> per_camera_uniform: PerCameraUniform;

{{include "rend3/vertex_attributes.wgsl"}}

struct ObjectRangeIndex {
    range: ObjectRange,
    index: u32,
}

var<workgroup> workgroup_object_range: ObjectRangeIndex;
var<workgroup> culling_results: array<atomic<u32>, 4>;

@compute @workgroup_size(256)
fn cs_main(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    if (lid.x == 0u) {
        let target_invocation = wid.x * 256u;
        // pulled directly from https://doc.rust-lang.org/src/core/slice/mod.rs.html#2412-2438

        var size = culling_job.total_objects;
        var left = 0u;
        var right = size;
        while left < right {
            let mid = left + size / 2u;

            let probe = culling_job.ranges[mid];

            if probe.invocation_end <= target_invocation {
                left = mid + 1u;
            } else if probe.invocation_start > target_invocation {
                right = mid;
            } else {
                workgroup_object_range = ObjectRangeIndex(probe, mid);
                atomicStore(&culling_results[0], 0u);
                atomicStore(&culling_results[1], 0u);
                atomicStore(&culling_results[2], 0u);
                atomicStore(&culling_results[3], 0u);
                break;
            }

            size = right - left;
        }
    }

    workgroupBarrier();

    let object_range = workgroup_object_range.range;
    let batch_object_index = workgroup_object_range.index;

    if gid.x >= object_range.invocation_end {
        if object_range.atomic_capable == 0u {
            atomicAdd(&primary_draw_calls[object_range.region_id].vertex_count, 3u);

            let global_output_invocation = culling_job.base_output_invocation + gid.x;

            primary_output[global_output_invocation * 3u + 0u] = INVALID_VERTEX;
            primary_output[global_output_invocation * 3u + 1u] = INVALID_VERTEX;
            primary_output[global_output_invocation * 3u + 2u] = INVALID_VERTEX;
        }
        return;
    }

    let invocation_within_object = gid.x - object_range.invocation_start;

    // If the first invocation in the region, set the region's draw call
    if object_range.local_region_id == 0u && invocation_within_object == 0u {
        primary_draw_calls[object_range.region_id].vertex_offset = 0;
        primary_draw_calls[object_range.region_id].instance_count = 1u;
        primary_draw_calls[object_range.region_id].base_instance = 0u;
        primary_draw_calls[object_range.region_id].base_index = (culling_job.base_output_invocation + gid.x) * 3u;
        secondary_draw_calls[object_range.region_id].vertex_offset = 0;
        secondary_draw_calls[object_range.region_id].instance_count = 1u;
        secondary_draw_calls[object_range.region_id].base_instance = 0u;
        secondary_draw_calls[object_range.region_id].base_index = (culling_job.base_output_invocation + gid.x) * 3u;
    }

    let index_0_index = invocation_within_object * 3u + 0u;
    let index_1_index = invocation_within_object * 3u + 1u;
    let index_2_index = invocation_within_object * 3u + 2u;

    let object = object_buffer[object_range.object_id];

    let index0 = vertex_buffer[object.first_index + index_0_index];
    let index1 = vertex_buffer[object.first_index + index_1_index];
    let index2 = vertex_buffer[object.first_index + index_2_index];

    let model_view_proj = per_camera_uniform.objects[object_range.object_id].model_view_proj;

    let position_start_offset = object.vertex_attribute_start_offsets[{{position_attribute_offset}}];
    let model_position0 = extract_attribute_vec3_f32(position_start_offset, index0);
    let model_position1 = extract_attribute_vec3_f32(position_start_offset, index1);
    let model_position2 = extract_attribute_vec3_f32(position_start_offset, index2);
    
    let position0 = model_view_proj * vec4<f32>(model_position0, 1.0);
    let position1 = model_view_proj * vec4<f32>(model_position1, 1.0);
    let position2 = model_view_proj * vec4<f32>(model_position2, 1.0);

    let det = determinant(mat3x3<f32>(position0.xyw, position1.xyw, position2.xyw));

    let passes_culling = det > 0.0;

    if object_range.atomic_capable == 1u {
        if passes_culling {
            let region_local_output_invocation = atomicAdd(&primary_draw_calls[object_range.region_id].vertex_count, 3u) / 3u;
            let job_local_output_invocation = region_local_output_invocation + object_range.region_base_invocation;
            let global_output_invocation = job_local_output_invocation + culling_job.base_output_invocation;

            primary_output[global_output_invocation * 3u + 0u] = pack_batch_index(batch_object_index, index0);
            primary_output[global_output_invocation * 3u + 1u] = pack_batch_index(batch_object_index, index1);
            primary_output[global_output_invocation * 3u + 2u] = pack_batch_index(batch_object_index, index2);
        } 
    } else {
        // TODO: remove this atomic
        atomicAdd(&primary_draw_calls[object_range.region_id].vertex_count, 3u);

        var output0 = INVALID_VERTEX;
        var output1 = INVALID_VERTEX;
        var output2 = INVALID_VERTEX;
        if passes_culling {
            output0 = pack_batch_index(batch_object_index, index0);
            output1 = pack_batch_index(batch_object_index, index1);
            output2 = pack_batch_index(batch_object_index, index2);
        }

        let global_output_invocation = culling_job.base_output_invocation + gid.x;

        primary_output[global_output_invocation * 3u + 0u] = pack_batch_index(batch_object_index, index0);
        primary_output[global_output_invocation * 3u + 1u] = pack_batch_index(batch_object_index, index1);
        primary_output[global_output_invocation * 3u + 2u] = pack_batch_index(batch_object_index, index2);
    }

    atomicOr(&culling_results[wid.x / 32u], u32(passes_culling) << (wid.x % 32u));

    workgroupBarrier();

    if wid.x == 0u {
        let global_invocation = culling_job.base_output_invocation + gid.x;
        current_culling_results[global_invocation / 32u + 0u] = atomicLoad(&culling_results[0]);
        current_culling_results[global_invocation / 32u + 1u] = atomicLoad(&culling_results[1]);
        current_culling_results[global_invocation / 32u + 2u] = atomicLoad(&culling_results[2]);
        current_culling_results[global_invocation / 32u + 3u] = atomicLoad(&culling_results[3]);
    }
}