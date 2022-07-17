{{include "structures.wgsl"}}
{{include "material.wgsl"}}

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
    @builtin(position) position: vec4<f32>,
    @location(0) view_position: vec4<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) tangent: vec3<f32>,
    @location(3) coords0: vec2<f32>,
    @location(4) coords1: vec2<f32>,
    @location(6) color: vec4<f32>,
    @location(7) @interpolate(flat) material: u32,
}

@group(0) @binding(0)
var primary_sampler: sampler;
@group(0) @binding(1)
var nearest_sampler: sampler;
@group(0) @binding(2)
var comparison_sampler: sampler_comparison; 
@group(0) @binding(3)
var<uniform> uniforms: UniformData;
@group(0) @binding(4)
var<storage> directional_lights: DirectionalLightData;
@group(0) @binding(5)
var shadows: texture_2d_array<f32>;

@group(1) @binding(0)
var<storage> object_output: array<ObjectOutputData>;

{{#if (eq profile "GpuDriven")}}
@group(1) @binding(1)
var<storage> materials: array<GpuMaterialData>;
@group(2) @binding(0)
var textures: binding_array<texture_2d<f32>>;
{{/if}}

{{#if (eq profile "CpuDriven")}}
@group(2) @binding(0)
var<storage> material: CpuMaterialData;
@group(2) @binding(1)
var albedo_tex: texture_2d<f32>;
@group(2) @binding(2)
var normal_tex: texture_2d<f32>;
@group(2) @binding(3)
var roughness_tex: texture_2d<f32>;
@group(2) @binding(4)
var metallic_tex: texture_2d<f32>;
@group(2) @binding(5)
var reflectance_tex: texture_2d<f32>;
@group(2) @binding(6)
var clear_coat_tex: texture_2d<f32>;
@group(2) @binding(7)
var clear_coat_roughness_tex: texture_2d<f32>;
@group(2) @binding(8)
var emissive_tex: texture_2d<f32>;
@group(2) @binding(9)
var anisotropy_tex: texture_2d<f32>;
@group(2) @binding(10)
var ambient_occlusion_tex: texture_2d<f32>;
{{/if}}

fn vs_main(vs_in: VertexInput) -> VertexOutput {
    let data = object_output[vs_in.object_idx];

    let position_vec4 = vec4<f32>(vs_in.position, 1.0);
    let mv_mat3 = mat3x3<f32>(data.model_view[0].xyz, data.model_view[1].xyz, data.model_view[2].xyz);

    var vs_out: VertexOutput;
    vs_out.material = data.material_idx;
    vs_out.view_position = data.model_view * position_vec4;
    vs_out.normal = normalize(mv_mat3 * (data.inv_scale_sq * vs_in.normal));
    vs_out.tangent = normalize(mv_mat3 * (data.inv_scale_sq * vs_in.tangent));
    vs_out.color = vs_in.color;
    vs_out.coords0 = vs_in.coords0;
    vs_out.coords1 = vs_in.coords1;
    vs_out.position = data.model_view_proj * position_vec4;

    return vs_out;
}

fn fs_main(vs_out: VertexOutput) -> @location(0) vec4<f32> {
    {{#if (eq profile "GpuDriven")}}
    let material = materials[vs_out.material];
    {{/if}}

    if (extract_material_flag(material.flags, FLAGS_UNLIT)) {
        
    }

    return vec4<f32>(0.0);
}
