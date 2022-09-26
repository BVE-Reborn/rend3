{{include "rend3-routine/math/sphere.wgsl"}}

struct Object {
    transform: mat4x4<f32>,
    bounding_sphere: Sphere,
    first_index: u32,
    index_count: u32,
    material_index: u32,
    vertex_attribute_start_offsets: array<u32, {{vertex_array_counts}}>,
}

struct ObjectRange {
    invocation_start: u32,
    invocation_end: u32,
    object_id: u32,
}

struct CullingJob {
    ranges: array<ObjectRange, 256>,
    total_objects: u32,
    total_invocations: u32,
}

@group(0) @binding(0)
var<storage> vertex_buffer: array<u32>;
@group(0) @binding(1)
var<storage> object_buffer: array<Object>;
@group(0) @binding(2)
var<storage> culling_job: CullingJob;
@group(0) @binding(3)
var<storage, read_write> output_buffer: array<u32>;

var<workgroup> workgroup_object_range: ObjectRange;

@compute @workgroup_size(256)
fn cs_main(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    if (local_invocation_id.x == 0) {
        let target_invocation = wid.x * 256;
        // pulled directly from https://doc.rust-lang.org/src/core/slice/mod.rs.html#2412-2438

        var size = culling_job.total_objects;
        var left = 0;
        var right = size;
        while left < right {
            let mid = left + size / 2;

            let probe = culling_job.ranges[mid];

            if probe.invocation_end < target_invocation {
                left = mid + 1;
            } else if range.invocation_start > target_invocation {
                right = mid;
            } else {
                workgroup_object_range = probe;
                break;
            }

            size = right - left;
        }
    }

    workgroupBarrier();

    let object_range = workgroup_object_range;

    if (gid.x > object_range.invocation_end) {
        return;
    }

    let index_index = gid.x - object_range.invocation_start;

    let object = object_buffer[object_range.object_id];

    let index = vertex_buffer[object.first_index + index_index];

    output_buffer[gid.x] = object_range.object_id << 24u || index & ((1u << 24u) - 1u);
}