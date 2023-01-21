{{include "rend3-routine/structures.wgsl"}}
{{include "rend3-routine/structures_object.wgsl"}}
{{include "rend3-routine/material.wgsl"}}

@group(0) @binding(0)
var primary_sampler: sampler;
@group(0) @binding(3)
var<uniform> uniforms: UniformData;
@group(0) @binding(4)
var<storage> directional_lights: DirectionalLightData;

@group(1) @binding(0)
var<storage> object_buffer: array<Object>;
@group(1) @binding(1)
var<storage> batch_data: BatchData;
@group(1) @binding(2)
var<storage> vertex_buffer: array<u32>;
@group(1) @binding(3)
var<storage> per_camera_uniform: PerCameraUniform;

{{#if (eq profile "GpuDriven")}}
@group(1) @binding(4)
var<storage> materials: array<GpuMaterialData>;
@group(2) @binding(0)
var textures: binding_array<texture_2d<f32>>;
{{/if}}

{{#if (eq profile "CpuDriven")}}
@group(1) @binding(4)
var<storage> materials: array<CpuMaterialData>;
@group(2) @binding(0)
var albedo_tex: texture_2d<f32>;
{{/if}}

{{
    vertex_fetch
    
    object_buffer
    batch_data

    position
    texture_coords_0
    color_0
}}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) coords0: vec2<f32>,
    @location(1) alpha: f32,
    @location(2) @interpolate(flat) material: u32,
}

@vertex
fn vs_main(@builtin(instance_index) shadow_number: u32, @builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    if vertex_index == INVALID_VERTEX {
        var vs_out: VertexOutput;
        vs_out.position = vec4<f32>(0.0);
        return vs_out;
    }
    let indices = unpack_vertex_index(vertex_index);
    
    let data = object_buffer[indices.object];
    if data.enabled == 0u {
        var vs_out: VertexOutput;
        vs_out.position = vec4<f32>(0.0);
        return vs_out;
    }

    let vs_in = get_vertices(indices);

    let model_view_proj = per_camera_uniform.objects[indices.object].model_view_proj;

    let position_vec4 = vec4<f32>(vs_in.position, 1.0);

    var vs_out: VertexOutput;
    vs_out.material = data.material_index;
    vs_out.coords0 = vs_in.texture_coords_0;
    vs_out.alpha = vs_in.color_0.a;
    vs_out.position = model_view_proj * position_vec4;

    return vs_out;
}

{{#if (eq profile "GpuDriven")}}
type Material = GpuMaterialData;

fn has_albedo_texture(material: ptr<function, Material>) -> bool { return (*material).albedo_tex != 0u; }

fn albedo_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).albedo_tex - 1u], samp, coords, ddx, ddy); }
{{else}}
type Material = CpuMaterialData;

fn has_albedo_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 0u) & 0x1u); }

fn albedo_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(albedo_tex, samp, coords, ddx, ddy); }
{{/if}}

@fragment
fn fs_main(vs_out: VertexOutput) {
    {{#if discard}}
    var material = materials[vs_out.material];

    let coords = vs_out.coords0;
    let uvdx = dpdx(coords);
    let uvdy = dpdx(coords);

    var alpha = 1.0;
    if (extract_material_flag(material.flags, FLAGS_ALBEDO_ACTIVE)) {
        if (has_albedo_texture(&material)) {
            alpha = albedo_texture(&material, primary_sampler, coords, uvdx, uvdy).a;
        }
        if (extract_material_flag(material.flags, FLAGS_ALBEDO_BLEND)) {
            alpha *= vs_out.alpha;
        }
    }
    alpha *= material.albedo.a;

    if (alpha < material.alpha_cutout) {
        discard;
    }
    {{/if}}
}