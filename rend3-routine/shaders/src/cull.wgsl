{{include "rend3-routine/structures.wgsl"}}
{{include "rend3-routine/structures_object.wgsl"}}

@group(0) @binding(0)
var<storage> vertex_buffer: array<u32>;
@group(0) @binding(1)
var<storage> object_buffer: array<Object>;

fn vertex_fetch(
    object_invocation: u32,
    object_info: ptr<function, ObjectCullingInformation>,
) -> Triangle {
    let index_0_index = object_invocation * 3u + 0u;
    let index_1_index = object_invocation * 3u + 1u;
    let index_2_index = object_invocation * 3u + 2u;

    let object = object_buffer[(*object_info).object_id];

    let index0 = vertex_buffer[object.first_index + index_0_index];
    let index1 = vertex_buffer[object.first_index + index_1_index];
    let index2 = vertex_buffer[object.first_index + index_2_index];

    let position_start_offset = object.vertex_attribute_start_offsets[{{position_attribute_offset}}];
    let model_position0 = extract_attribute_vec3_f32(position_start_offset, index0);
    let model_position1 = extract_attribute_vec3_f32(position_start_offset, index1);
    let model_position2 = extract_attribute_vec3_f32(position_start_offset, index2);

    return Triangle(
        TriangleVertices(model_position0, model_position1, model_position2),
        TriangleIndices(index0, index1, index2)
    );
}

@group(0) @binding(2)
var<storage> culling_job: BatchData;

struct DrawCallBuffer {
    /// We always put the buffer that needs to be present in the next frame first.
    predicted_object_offset: u32,
    residual_object_offset: u32,
    calls: array<IndirectCall>,
}

@group(0) @binding(3)
var<storage, read_write> draw_calls: DrawCallBuffer;

fn init_draw_calls(global_invocation: u32, region_id: u32) {
    // Init the inheritable draw call
    let predicted_object_draw_index = draw_calls.predicted_object_offset + region_id;
    draw_calls.calls[predicted_object_draw_index].vertex_offset = 0;
    draw_calls.calls[predicted_object_draw_index].instance_count = 1u;
    draw_calls.calls[predicted_object_draw_index].base_instance = 0u;
    draw_calls.calls[predicted_object_draw_index].base_index = global_invocation * 3u;

    // Init the residual objects draw call
    let residual_object_draw_index = draw_calls.residual_object_offset + region_id;
    draw_calls.calls[residual_object_draw_index].vertex_offset = 0;
    draw_calls.calls[residual_object_draw_index].instance_count = 1u;
    draw_calls.calls[residual_object_draw_index].base_instance = 0u;
    draw_calls.calls[residual_object_draw_index].base_index = global_invocation * 3u;
}

fn add_predicted_triangle_to_draw_call(region_id: u32) -> u32 {
    let output_region_index = atomicAdd(&draw_calls.calls[draw_calls.predicted_object_offset + region_id].vertex_count, 3u);
    let output_region_triangle = output_region_index / 3u;
    return output_region_triangle;
}

fn add_residual_triangle_to_draw_call(region_id: u32) -> u32 {
    let output_region_index = atomicAdd(&draw_calls.calls[draw_calls.residual_object_offset + region_id].vertex_count, 3u);
    let output_region_triangle = output_region_index / 3u;
    return output_region_triangle;
}

struct OutputIndexBuffer {
    /// We always put the buffer that needs to be present in the next frame first.
    predicted_object_offset: u32,
    residual_object_offset: u32,
    indices: array<u32>,
}
@group(0) @binding(4)
var<storage, read_write> output_indices : OutputIndexBuffer;

fn write_predicted_atomic_triangle(
    batch_object_index: u32,
    object_info: ptr<function, ObjectCullingInformation>,
    indices: TriangleIndices,
) {
    let region_invocation = add_predicted_triangle_to_draw_call((*object_info).region_id);
    let batch_invocation = region_invocation + (*object_info).region_base_invocation;
    let global_invocation = batch_invocation + culling_job.batch_base_invocation;

    let packed_indices = pack_batch_indices(batch_object_index, indices);

    let predicted_object_indices_index = output_indices.predicted_object_offset + global_invocation * 3u;
    output_indices.indices[predicted_object_indices_index] = packed_indices[0];
    output_indices.indices[predicted_object_indices_index + 1u] = packed_indices[1];
    output_indices.indices[predicted_object_indices_index + 2u] = packed_indices[2];
}

fn write_residual_atomic_triangle(
    batch_object_index: u32,
    object_info: ptr<function, ObjectCullingInformation>,
    indices: TriangleIndices,
) {
    let region_invocation = add_residual_triangle_to_draw_call((*object_info).region_id);
    let batch_invocation = region_invocation + (*object_info).region_base_invocation;
    let global_invocation = batch_invocation + culling_job.batch_base_invocation;

    let packed_indices = pack_batch_indices(batch_object_index, indices);

    let residual_object_indices_index = output_indices.residual_object_offset + global_invocation * 3u;
    output_indices.indices[residual_object_indices_index] = packed_indices[0];
    output_indices.indices[residual_object_indices_index + 1u] = packed_indices[1];
    output_indices.indices[residual_object_indices_index + 2u] = packed_indices[2];
}

fn write_residual_nonatomic_triangle(
    invocation: u32,
    batch_object_index: u32,
    object_info: ptr<function, ObjectCullingInformation>,
    indices: TriangleIndices,
) {
    add_residual_triangle_to_draw_call((*object_info).region_id);

    let packed_indices = pack_batch_indices(batch_object_index, indices);

    let residual_object_indices_index = output_indices.residual_object_offset + invocation * 3u;
    output_indices.indices[residual_object_indices_index] = packed_indices[0];
    output_indices.indices[residual_object_indices_index + 1u] = packed_indices[1];
    output_indices.indices[residual_object_indices_index + 2u] = packed_indices[2];
}

fn write_invalid_residual_nonatomic_triangle(invocation: u32, object_info: ptr<function, ObjectCullingInformation>) {
    add_residual_triangle_to_draw_call((*object_info).region_id);

    let residual_object_indices_index = output_indices.residual_object_offset + invocation * 3u;
    output_indices.indices[residual_object_indices_index] = INVALID_VERTEX;
    output_indices.indices[residual_object_indices_index + 1u] = INVALID_VERTEX;
    output_indices.indices[residual_object_indices_index + 2u] = INVALID_VERTEX;
}

struct CullingResults {
    /// We always put the buffer that needs to be present in the next frame first.
    output_offset: u32,
    input_offset: u32,
    bits: array<u32>,
}
@group(0) @binding(5)
var<storage, read_write> culling_results: CullingResults;

fn get_previous_culling_result(object_info: ptr<function, ObjectCullingInformation>, object_invocation: u32) -> bool {
    if (*object_info).previous_global_invocation == 0xFFFFFFFFu {
        return false;
    }

    let previous_global_invocation = object_invocation + (*object_info).previous_global_invocation;
    let bitmask = culling_results.bits[culling_results.input_offset + (previous_global_invocation / 32u)];
    return ((bitmask >> (previous_global_invocation % 32u)) & 0x1u) == 0x1u;
}

@group(0) @binding(6)
var<storage> per_camera_uniform: PerCameraUniform;

fn is_shadow_pass() -> bool {
    return per_camera_uniform.shadow_index != 0xFFFFFFFFu;
}

@group(0) @binding(7)
var hirearchical_z_buffer: texture_depth_2d;
@group(0) @binding(8)
var nearest_sampler: sampler;

{{include "rend3/vertex_attributes.wgsl"}}

struct ObjectSearchResult {
    range: ObjectCullingInformation,
    index_within_region: u32,
}

fn find_object_info(wid: u32) -> ObjectSearchResult {
    let target_invocation = wid * 64u;
    // pulled directly from https://doc.rust-lang.org/src/core/slice/mod.rs.html#2412-2438

    var size = culling_job.total_objects;
    var left = 0u;
    var right = size;
    var object_info: ObjectCullingInformation;
    while left < right {
        let mid = left + size / 2u;

        let probe = culling_job.object_culling_information[mid];

        if probe.invocation_end <= target_invocation {
            left = mid + 1u;
        } else if probe.invocation_start > target_invocation {
            right = mid;
        } else {
            return ObjectSearchResult(probe, mid);
        }

        size = right - left;
    }
    
    // This is unreachable, but required for the compiler to be happy
    return ObjectSearchResult(object_info, 0xFFFFFFFFu);
}

// 64 workgroup size / 32 bits
var<workgroup> workgroup_culling_results: array<atomic<u32>, 2>;

fn clear_culling_results(lid: u32) {
    if lid == 0u {
        atomicStore(&workgroup_culling_results[0], 0u);
        atomicStore(&workgroup_culling_results[1], 0u);
    }
}

fn save_culling_results(global_invocation: u32, lid: u32, passed_culling: bool) {
    atomicOr(&workgroup_culling_results[lid / 32u], u32(passed_culling) << (lid % 32u));

    workgroupBarrier();

    if lid == 0u {
        let culling_results_index = culling_results.output_offset + (global_invocation / 32u);
        culling_results.bits[culling_results_index + 0u] = atomicLoad(&workgroup_culling_results[0]);
        culling_results.bits[culling_results_index + 1u] = atomicLoad(&workgroup_culling_results[1]);
    }
}

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

    var minval = textureLoad(texture, top_left, int_mipmap);
    minval = min(minval, textureLoad(texture, top_right, int_mipmap));
    minval = min(minval, textureLoad(texture, bottom_left, int_mipmap));
    minval = min(minval, textureLoad(texture, bottom_right, int_mipmap));
    return minval;
}

fn execute_culling(
    model_view_proj: mat4x4<f32>,
    vertices: TriangleVertices,
) -> bool {
    let position0 = model_view_proj * vec4<f32>(vertices[0], 1.0);
    let position1 = model_view_proj * vec4<f32>(vertices[1], 1.0);
    let position2 = model_view_proj * vec4<f32>(vertices[2], 1.0);

    let det = determinant(mat3x3<f32>(position0.xyw, position1.xyw, position2.xyw));

    if (per_camera_uniform.flags & PCU_FLAGS_AREA_VISIBLE_MASK) == PCU_FLAGS_POSITIVE_AREA_VISIBLE && det <= 0.0 {
        return false;
    }
    if (per_camera_uniform.flags & PCU_FLAGS_AREA_VISIBLE_MASK) == PCU_FLAGS_NEGATIVE_AREA_VISIBLE && det >= 0.0 {
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

    if (per_camera_uniform.flags & PCU_FLAGS_MULTISAMPLE_MASK) == PCU_FLAGS_MULTISAMPLE_DISABLED {
        let misses_pixel_center = any(round(min_screen_xy) == round(max_screen_xy));

        if misses_pixel_center {
            return false;
        }
    }

    // We skip hi-z calculation if we're doing a shadow pass
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

@compute @workgroup_size(64)
fn cs_main(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_id) lid: vec3<u32>,
) {
    clear_culling_results(lid.x);

    let object_search_results = find_object_info(wid.x);
    var object_info = object_search_results.range;
    let batch_object_index = object_search_results.index_within_region;
    let global_invocation = culling_job.batch_base_invocation + gid.x;

    if gid.x >= object_info.invocation_end {
        if object_info.atomic_capable == 0u {
            write_invalid_residual_nonatomic_triangle(global_invocation, &object_info);
        }
        return;
    }

    let object_invocation = gid.x - object_info.invocation_start;

    // If the first invocation in the region, set the region's draw call
    if object_info.local_region_id == 0u && object_invocation == 0u {
        init_draw_calls(global_invocation, object_info.region_id);
    }

    let triangle = vertex_fetch(object_invocation, &object_info);

    let model_view_proj = per_camera_uniform.objects[object_info.object_id].model_view_proj;

    let passes_culling = execute_culling(model_view_proj, triangle.vertices);

    if object_info.atomic_capable == 1u {
        if passes_culling {
            write_predicted_atomic_triangle(batch_object_index, &object_info, triangle.indices);

            if !is_shadow_pass() {
                let previously_passed_culling = get_previous_culling_result(&object_info, object_invocation);

                if !previously_passed_culling {
                    write_residual_atomic_triangle(batch_object_index, &object_info, triangle.indices);
                }
            }
        }
    } else {
        if passes_culling {
            write_residual_nonatomic_triangle(global_invocation, batch_object_index, &object_info, triangle.indices);
        } else {
            write_invalid_residual_nonatomic_triangle(global_invocation, &object_info);
        }
    }

    save_culling_results(global_invocation, lid.x, passes_culling);
}
