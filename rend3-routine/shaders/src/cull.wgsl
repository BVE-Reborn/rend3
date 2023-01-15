{{include "rend3-routine/structures.wgsl"}}
{{include "rend3-routine/structures_object.wgsl"}}

@group(0) @binding(0)
var<storage> vertex_buffer: array<u32>;
@group(0) @binding(1)
var<storage> object_buffer: array<Object>;
@group(0) @binding(2)
var<storage> culling_job: BatchData;
@group(0) @binding(3)
var<storage, read_write> output_buffer: array<u32>;

struct ObjectRangeIndex {
    range: ObjectRange,
    index: u32,
}

var<workgroup> workgroup_object_range: ObjectRangeIndex;

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
                break;
            }

            size = right - left;
        }
    }

    workgroupBarrier();

    let object_range = workgroup_object_range.range;
    let local_object_index = workgroup_object_range.index;

    if (gid.x >= object_range.invocation_end) {
        output_buffer[(culling_job.base_output_invocation + gid.x) * 3u + 0u] = 0x00FFFFFFu;
        output_buffer[(culling_job.base_output_invocation + gid.x) * 3u + 1u] = 0x00FFFFFFu;
        output_buffer[(culling_job.base_output_invocation + gid.x) * 3u + 2u] = 0x00FFFFFFu;
        return;
    }

    let index_0_index = (gid.x - object_range.invocation_start) * 3u + 0u;
    let index_1_index = (gid.x - object_range.invocation_start) * 3u + 1u;
    let index_2_index = (gid.x - object_range.invocation_start) * 3u + 2u;

    let object = object_buffer[object_range.object_id];

    let index0 = vertex_buffer[object.first_index + index_0_index];
    let index1 = vertex_buffer[object.first_index + index_1_index];
    let index2 = vertex_buffer[object.first_index + index_2_index];

    output_buffer[(culling_job.base_output_invocation + gid.x) * 3u + 0u] = local_object_index << 24u | index0 & ((1u << 24u) - 1u);
    output_buffer[(culling_job.base_output_invocation + gid.x) * 3u + 1u] = local_object_index << 24u | index1 & ((1u << 24u) - 1u);
    output_buffer[(culling_job.base_output_invocation + gid.x) * 3u + 2u] = local_object_index << 24u | index2 & ((1u << 24u) - 1u);
}