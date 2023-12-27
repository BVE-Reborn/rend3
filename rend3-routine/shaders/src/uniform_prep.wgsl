{{include "rend3-routine/structures.wgsl"}}
{{include "rend3-routine/structures_object.wgsl"}}

@group(0) @binding(0)
var<storage> object_buffer: array<Object>;
@group(0) @binding(1)
var<storage, read_write> per_camera_uniform: PerCameraUniform;

@compute @workgroup_size(256)
fn cs_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
) {
    let idx = gid.x;

    if idx >= per_camera_uniform.object_count {
        return;
    }
    if object_buffer[idx].enabled == 0u {
        return;
    }

    let model_view = per_camera_uniform.view * object_buffer[idx].transform;
    let model_view_proj = per_camera_uniform.view_proj * object_buffer[idx].transform;

    per_camera_uniform.objects[idx].model_view = model_view;
    per_camera_uniform.objects[idx].model_view_proj = model_view_proj;
}
