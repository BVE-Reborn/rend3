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
@group(0) @binding(10)
var hirearchical_z_buffer: texture_depth_2d;
@group(0) @binding(11)
var nearest_sampler: sampler;

{{include "rend3/vertex_attributes.wgsl"}}

struct ObjectRangeIndex {
    range: ObjectRange,
    index: u32,
}

var<workgroup> workgroup_object_range: ObjectRangeIndex;
// 256 workgroup size / 32 bits
var<workgroup> culling_results: array<atomic<u32>, 8>;

fn textureSampleMin(texture: texture_depth_2d, uv: vec2<f32>, mipmap: f32) -> f32 {
    let int_mipmap = i32(mipmap);
    let mip_resolution = vec2<f32>(textureDimensions(texture, int_mipmap).xy);

    let pixel_coords = uv * mip_resolution - 0.5;

    let low = vec2<u32>(max(floor(pixel_coords), vec2<f32>(0.0)));
    let high = vec2<u32>(min(ceil(pixel_coords), mip_resolution - 1.0));

    let top_left = vec2<u32>(low.x, low.y);
    let top_right = vec2<u32>(high.x, low.y);
    let bottom_left = vec2<u32>(low.x, high.y);
    let bottom_right = vec2<u32>(high.x, high.y);

    var minval = 1.0;
    minval = min(minval, textureLoad(texture, top_left, int_mipmap));
    minval = min(minval, textureLoad(texture, top_right, int_mipmap));
    minval = min(minval, textureLoad(texture, bottom_left, int_mipmap));
    minval = min(minval, textureLoad(texture, bottom_right, int_mipmap));
    return minval;
}

fn execute_culling(
    model_view_proj: mat4x4<f32>,
    model_position0: vec3<f32>,
    model_position1: vec3<f32>,
    model_position2: vec3<f32>
) -> bool {
    let position0 = model_view_proj * vec4<f32>(model_position0, 1.0);
    let position1 = model_view_proj * vec4<f32>(model_position1, 1.0);
    let position2 = model_view_proj * vec4<f32>(model_position2, 1.0);

    let det = determinant(mat3x3<f32>(position0.xyw, position1.xyw, position2.xyw));

    if det <= 0.0 {
        return false;
    }

    let ndc0 = position0.xyz / position0.w;
    let ndc1 = position1.xyz / position1.w;
    let ndc2 = position2.xyz / position2.w;

    let min_ndc_xy = min(ndc0.xy, min(ndc1.xy, ndc2.xy));
    let max_ndc_xy = max(ndc0.xy, max(ndc1.xy, ndc2.xy));

    let half_res = per_camera_uniform.resolution / 2.0;
    let min_screen_xy = (min_ndc_xy + 1.0) * half_res;
    let max_screen_xy = (max_ndc_xy + 1.0) * half_res;

    let misses_pixel_center = any(round(min_screen_xy) == round(max_screen_xy));

    if misses_pixel_center {
        return false;
    }

    if per_camera_uniform.shadow_index != 0xFFFFFFFFu {
        return true;
    }

    var min_tex_coords = (min_ndc_xy + 1.0) / 2.0;
    var max_tex_coords = (max_ndc_xy + 1.0) / 2.0;
    min_tex_coords.y = 1.0 - min_tex_coords.y;
    max_tex_coords.y = 1.0 - max_tex_coords.y;

    let uv = (max_tex_coords + min_tex_coords) / 2.0;
    let edges = max_screen_xy - min_screen_xy;

    let longest_edge = max(edges.x, edges.y);
    let mip = ceil(log2(max(longest_edge, 1.0)));

    let depth = max(max(ndc0.z, ndc1.z), ndc2.z);
    let occlusion_depth = textureSampleMin(hirearchical_z_buffer, uv, mip);

    if depth < occlusion_depth {
        return false;
    }

    return true;
}

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
                atomicStore(&culling_results[4], 0u);
                atomicStore(&culling_results[5], 0u);
                atomicStore(&culling_results[6], 0u);
                atomicStore(&culling_results[7], 0u);
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

    let passes_culling = execute_culling(model_view_proj, model_position0, model_position1, model_position2);

    if object_range.atomic_capable == 1u {
        if passes_culling {
            if per_camera_uniform.shadow_index == 0xFFFFFFFFu {
                let region_local_output_invocation = atomicAdd(&primary_draw_calls[object_range.region_id].vertex_count, 3u) / 3u;
                let job_local_output_invocation = region_local_output_invocation + object_range.region_base_invocation;
                let global_output_invocation = job_local_output_invocation + culling_job.base_output_invocation;

                primary_output[global_output_invocation * 3u + 0u] = pack_batch_index(batch_object_index, index0);
                primary_output[global_output_invocation * 3u + 1u] = pack_batch_index(batch_object_index, index1);
                primary_output[global_output_invocation * 3u + 2u] = pack_batch_index(batch_object_index, index2);
            }
        
            let previous_global_invocation = invocation_within_object + object_range.previous_global_invocation;

            let previously_passed_culling = ((previous_culling_results[previous_global_invocation / 32u] >> (previous_global_invocation % 32u)) & 0x1u) == 1u;

            if !previously_passed_culling || per_camera_uniform.shadow_index != 0xFFFFFFFFu {
                let region_local_secondary_invocation = atomicAdd(&secondary_draw_calls[object_range.region_id].vertex_count, 3u) / 3u;
                let job_local_secondary_invocation = region_local_secondary_invocation + object_range.region_base_invocation;
                let global_secondary_invocation = job_local_secondary_invocation + culling_job.base_output_invocation;

                secondary_output[global_secondary_invocation * 3u + 0u] = pack_batch_index(batch_object_index, index0);
                secondary_output[global_secondary_invocation * 3u + 1u] = pack_batch_index(batch_object_index, index1);
                secondary_output[global_secondary_invocation * 3u + 2u] = pack_batch_index(batch_object_index, index2);
            }
        }
    } else {
        // TODO: remove this atomic
        atomicAdd(&secondary_draw_calls[object_range.region_id].vertex_count, 3u);

        var output0 = INVALID_VERTEX;
        var output1 = INVALID_VERTEX;
        var output2 = INVALID_VERTEX;
        if passes_culling {
            output0 = pack_batch_index(batch_object_index, index0);
            output1 = pack_batch_index(batch_object_index, index1);
            output2 = pack_batch_index(batch_object_index, index2);
        }

        let global_output_invocation = culling_job.base_output_invocation + gid.x;

        secondary_output[global_output_invocation * 3u + 0u] = pack_batch_index(batch_object_index, index0);
        secondary_output[global_output_invocation * 3u + 1u] = pack_batch_index(batch_object_index, index1);
        secondary_output[global_output_invocation * 3u + 2u] = pack_batch_index(batch_object_index, index2);

        // TODO: We assume here that all non-atomic capable triangles are rendered _after_ culling.
    }

    atomicOr(&culling_results[lid.x / 32u], u32(passes_culling) << (lid.x % 32u));

    workgroupBarrier();

    if lid.x == 0u {
        let global_invocation = culling_job.base_output_invocation + gid.x;
        current_culling_results[global_invocation / 32u + 0u] = atomicLoad(&culling_results[0]);
        current_culling_results[global_invocation / 32u + 1u] = atomicLoad(&culling_results[1]);
        current_culling_results[global_invocation / 32u + 2u] = atomicLoad(&culling_results[2]);
        current_culling_results[global_invocation / 32u + 3u] = atomicLoad(&culling_results[3]);
        current_culling_results[global_invocation / 32u + 4u] = atomicLoad(&culling_results[4]);
        current_culling_results[global_invocation / 32u + 5u] = atomicLoad(&culling_results[5]);
        current_culling_results[global_invocation / 32u + 6u] = atomicLoad(&culling_results[6]);
        current_culling_results[global_invocation / 32u + 7u] = atomicLoad(&culling_results[7]);
    }
}