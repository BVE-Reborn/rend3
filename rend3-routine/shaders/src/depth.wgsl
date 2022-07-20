{{include "structures.wgsl"}}

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec3<f32>,
    @location(3) coords0: vec2<f32>,
    @location(4) coords1: vec2<f32>,
    @location(5) color: vec4<f32>,
{{#if (eq profile "GpuDriven")}}
    @location(8) object_idx: u32,
{{else}}
    @builtin(instance_index) object_idx: u32,
{{/if}}
}

struct VertexOutput {
    @builtin(position) position0: vec4<f32>,
    @location(0) position1: vec4<f32>,
    @location(1) coords0: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) material: u32,
}

@group(0) @binding(0)
var primary_sampler: sampler;

@group(1) @binding(0)
var<storage> object_output: array<ObjectOutputData>;

{{#if (eq profile "GpuDriven")}}
    @group(1) @binding(1)
    var<storage> material_data: array<f32>;
    @group(3) @binding(0)
    var textures: binding_array<texture_2d<f32>>;
{{else}}
    @group(3) @binding(0)
    var<storage> material_data: array<f32>;
    @group(3) @binding(1)
    var texture: texture_2d<f32>;
{{/if}}

struct DataAbi {
    stride: u32, // Stride in offset into a float array (i.e. byte index / 4). Unused when GpuDriven.
    texture_offset: u32, // Must be zero when GpuDriven. When GpuDriven, it's the index into the material data with the texture enable bitflag.
    cutoff_offset: u32, // Stride in offset into a float array  (i.e. byte index / 4)
    uv_transform_offset: u32, // Stride in offset into a float array pointing to a mat3 with the uv transform (i.e. byte index / 4). 0xFFFFFFFF represents "no transform"
}

@group(2) @binding(0)
var<uniform> abi: DataAbi;

fn vs_main(vs_in: VertexInput) -> VertexOutput {
    let data = object_output[vs_in.object_idx];

    let position = data.model_view_proj * vec4<f32>(vs_in.position, 1.0);

    var vs_out: VertexOutput;
    vs_out.position0 = position;
    vs_out.position1 = position;
    vs_out.material = data.material_idx;
    vs_out.color = vs_in.color;
    vs_out.coords0 = vs_in.coords0;

    return vs_out;
}

fn fs_cutout(vs_out: VertexOutput) {
    let base_material_offset = abi.stride * vs_out.material;
    let cutoff = material_data[base_material_offset + abi.cutoff_offset];

    var coords: vec2<f32>;
    if (abi.uv_transform_offset != 0xFFFFFFFFu) {
        let base_transform_offset = base_material_offset + abi.uv_transform_offset;
        let transform = mat3x3<f32>(
            material_data[base_transform_offset + 0u],
            material_data[base_transform_offset + 1u],
            material_data[base_transform_offset + 2u],
            material_data[base_transform_offset + 4u],
            material_data[base_transform_offset + 5u],
            material_data[base_transform_offset + 6u],
            material_data[base_transform_offset + 8u],
            material_data[base_transform_offset + 9u],
            material_data[base_transform_offset + 10u],
        );
        coords = (transform * vec3<f32>(vs_out.coords0, 1.0)).xy;
    } else {
        coords = vs_out.coords0;
    }

    let uvdx = dpdx(coords);
    let uvdy = dpdy(coords);

    {{#if (eq profile "GpuDriven")}}
    let texture_index = bitcast<u32>(material_data[base_material_offset + abi.texture_offset]);
    if (texture_index != 0u) {
        let alpha = textureSampleGrad(textures[texture_index - 1u], primary_sampler, coords, uvdx, uvdy).a;

        if (alpha <= cutoff) {
            discard;
        }
    }
    {{else}}
    let texture_enable_bitflags = bitcast<u32>(material_data[base_material_offset + abi.texture_offset]);
    if (bool(texture_enable_bitflags & 0x1u)) {
        let alpha = textureSampleGrad(texture, primary_sampler, coords, uvdx, uvdy).a;

        if (alpha <= cutoff) {
            discard;
        }
    }
    {{/if}}
}

fn fs_no_cutout(vs_out: VertexOutput) {

}
