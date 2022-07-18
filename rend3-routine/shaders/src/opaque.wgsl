{{include "structures.wgsl"}}
{{include "material.wgsl"}}
{{include "math/color.wgsl"}}

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

{{#if (eq profile "GpuDriven")}}
type Material = GpuMaterialData;

fn has_albedo_texture(material: ptr<function, Material>) -> bool { return (*material).albedo_tex != 0u; }
fn has_normal_texture(material: ptr<function, Material>) -> bool { return (*material).normal_tex != 0u; }
fn has_roughness_texture(material: ptr<function, Material>) -> bool { return (*material).roughness_tex != 0u; }
fn has_metallic_texture(material: ptr<function, Material>) -> bool { return (*material).metallic_tex != 0u; }
fn has_reflectance_texture(material: ptr<function, Material>) -> bool { return (*material).reflectance_tex != 0u; }
fn has_clear_coat_texture(material: ptr<function, Material>) -> bool { return (*material).clear_coat_tex != 0u; }
fn has_clear_coat_roughness_texture(material: ptr<function, Material>) -> bool { return (*material).clear_coat_roughness_tex != 0u; }
fn has_emissive_texture(material: ptr<function, Material>) -> bool { return (*material).emissive_tex != 0u; }
fn has_anisotropy_texture(material: ptr<function, Material>) -> bool { return (*material).anisotropy_tex != 0u; }
fn has_ambient_occlusion_texture(material: ptr<function, Material>) -> bool { return (*material).ambient_occlusion_tex != 0u; }

fn albedo_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).albedo_tex - 1u], samp, coords, ddx, ddy); }
fn normal_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).normal_tex - 1u], samp, coords, ddx, ddy); }
fn roughness_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).roughness_tex - 1u], samp, coords, ddx, ddy); }
fn metallic_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).metallic_tex - 1u], samp, coords, ddx, ddy); }
fn reflectance_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).reflectance_tex - 1u], samp, coords, ddx, ddy); }
fn clear_coat_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).clear_coat_tex - 1u], samp, coords, ddx, ddy); }
fn clear_coat_roughness_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).clear_coat_roughness_tex - 1u], samp, coords, ddx, ddy); }
fn emissive_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).emissive_tex - 1u], samp, coords, ddx, ddy); }
fn anisotropy_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).anisotropy_tex - 1u], samp, coords, ddx, ddy); }
fn ambient_occlusion_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(textures[(*material).ambient_occlusion_tex - 1u], samp, coords, ddx, ddy); }
{{else}}
type Material = CpuMaterialData;

fn has_albedo_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 0u) & 0x1u); }
fn has_normal_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 1u) & 0x1u); }
fn has_roughness_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 2u) & 0x1u); }
fn has_metallic_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 3u) & 0x1u); }
fn has_reflectance_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 4u) & 0x1u); }
fn has_clear_coat_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 5u) & 0x1u); }
fn has_clear_coat_roughness_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 6u) & 0x1u); }
fn has_emissive_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 7u) & 0x1u); }
fn has_anisotropy_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 8u) & 0x1u); }
fn has_ambient_occlusion_texture(material: ptr<function, Material>) -> bool { return bool(((*material).texture_enable >> 9u) & 0x1u); }

fn albedo_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(albedo_tex, samp, coords, ddx, ddy); }
fn normal_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(normal_tex, samp, coords, ddx, ddy); }
fn roughness_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(roughness_tex, samp, coords, ddx, ddy); }
fn metallic_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(metallic_tex, samp, coords, ddx, ddy); }
fn reflectance_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(reflectance_tex, samp, coords, ddx, ddy); }
fn clear_coat_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(clear_coat_tex, samp, coords, ddx, ddy); }
fn clear_coat_roughness_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(clear_coat_roughness_tex, samp, coords, ddx, ddy); }
fn emissive_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(emissive_tex, samp, coords, ddx, ddy); }
fn anisotropy_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(anisotropy_tex, samp, coords, ddx, ddy); }
fn ambient_occlusion_texture(material: ptr<function, Material>, samp: sampler, coords: vec2<f32>, ddx: vec2<f32>, ddy: vec2<f32>) -> vec4<f32> { return textureSampleGrad(ambient_occlusion_tex, samp, coords, ddx, ddy); }
{{/if}}

fn get_pixel_data(material: Material, s: sampler, vs_out: VertexOutput) -> PixelData {
    var material = material;
    var pixel: PixelData;

    let coords = (material.uv_transform0 * vec3<f32>(vs_out.coords0, 1.0)).xy;
    let uvdx = dpdx(coords);
    let uvdy = dpdy(coords);

    // --- ALBEDO ---

    if (extract_material_flag(material.flags, FLAGS_ALBEDO_ACTIVE)) {
        if (has_albedo_texture(&material)) {
            pixel.albedo = albedo_texture(&material, s, coords, uvdx, uvdy);
        } else {
            pixel.albedo = vec4<f32>(1.0);
        }
        if (extract_material_flag(material.flags, FLAGS_ALBEDO_BLEND)) {
            if (extract_material_flag(material.flags, FLAGS_ALBEDO_VERTEX_SRGB)) {
                pixel.albedo *= vec4<f32>(srgb_display_to_scene(vs_out.color.rgb), vs_out.color.a);
            } else {
                pixel.albedo *= vs_out.color;
            }
        }
    } else {
        pixel.albedo = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    pixel.albedo *= material.albedo;

    // --- STOP IF UNLIT ---

    if (extract_material_flag(material.flags, FLAGS_UNLIT)) {
        pixel.normal = normalize(vs_out.normal);
        return pixel;
    }

    // --- NORMAL TEXTURE ---

    if (has_normal_texture(&material)) {
        let texture_read = normal_texture(&material, s, coords, uvdx, uvdy);
        var normal: vec3<f32>;
        if (extract_material_flag(material.flags, FLAGS_BICOMPONENT_NORMAL)) {
            var bicomp: vec2<f32>;
            if (extract_material_flag(material.flags, FLAGS_SWIZZLED_NORMAL)) {
                bicomp = texture_read.ag;
            } else {
                bicomp = texture_read.rg;
            }
            bicomp = bicomp * 2.0 - 1.0;
            let bicomp_sq = bicomp * bicomp;

            normal = vec3<f32>(bicomp, sqrt(1.0 - bicomp_sq.r - bicomp_sq.g));
        } else {
            normal = normalize(texture_read.rgb * 2.0 - 1.0);
        }
        if (extract_material_flag(material.flags, FLAGS_YDOWN_NORMAL)) {
            normal.y = -normal.y;
        }
        let normal_norm = normalize(vs_out.normal);
        let tangent_norm = normalize(vs_out.tangent);
        let bitangent = cross(normal_norm, tangent_norm);

        let tbn = mat3x3(tangent_norm, bitangent, normal_norm);

        pixel.normal = tbn * normal;
    } else {
        pixel.normal = vs_out.normal;
    }
    pixel.normal = normalize(pixel.normal);

    // --- AO, Metallic, and Roughness ---

    if (extract_material_flag(material.flags, FLAGS_AOMR_COMBINED)) {
        // In roughness texture:
        // Red: AO
        // Green: Roughness
        // Blue: Metallic
        if (has_roughness_texture(&material)) {
            let aomr = roughness_texture(&material, s, coords, uvdx, uvdy);
            pixel.ambient_occlusion = material.ambient_occlusion * aomr[0];
            pixel.perceptual_roughness = material.roughness * aomr[1];
            pixel.metallic = material.metallic * aomr[2];
        } else {
            pixel.ambient_occlusion = material.ambient_occlusion;
            pixel.perceptual_roughness = material.roughness;
            pixel.metallic = material.metallic;
        }
    } else if (extract_material_flag(material.flags, FLAGS_AOMR_SWIZZLED_SPLIT) || extract_material_flag(material.flags, FLAGS_AOMR_SPLIT)) {
        // In ao texture:
        // Red: AO
        //
        // In roughness texture (FLAGS_AOMR_SPLIT):
        // Green: Roughness
        // Blue: Metallic
        //
        // In roughness texture (FLAGS_AOMR_SWIZZLED_SPLIT):
        // Red: Roughness
        // Green: Metallic
        if (has_roughness_texture(&material)) {
            let texture_read = roughness_texture(&material, s, coords, uvdx, uvdy);
            var mr: vec2<f32>;
            if (extract_material_flag(material.flags, FLAGS_AOMR_SWIZZLED_SPLIT)) {
                mr = texture_read.gb;
            } else {
                mr = texture_read.rg;
            }
            pixel.perceptual_roughness = material.roughness * mr[0];
            pixel.metallic = material.metallic * mr[1];
        } else {
            pixel.perceptual_roughness = material.roughness;
            pixel.metallic = material.metallic;
        }

        if (has_ambient_occlusion_texture(&material)) {
            let texture_read = ambient_occlusion_texture(&material, s, coords, uvdx, uvdy);
            pixel.ambient_occlusion = material.ambient_occlusion * texture_read.r;
        } else {
            pixel.ambient_occlusion = material.ambient_occlusion;
        }
    }

    return pixel;
}

fn fs_main(vs_out: VertexOutput) -> @location(0) vec4<f32> {
    {{#if (eq profile "GpuDriven")}}
    let material = materials[vs_out.material];
    {{/if}}

    if (extract_material_flag(material.flags, FLAGS_UNLIT)) {
        
    }

    return vec4<f32>(0.0);
}
