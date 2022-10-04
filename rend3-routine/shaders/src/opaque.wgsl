{{include "rend3-routine/structures.wgsl"}}
{{include "rend3-routine/material.wgsl"}}
{{include "rend3-routine/math/brdf.wgsl"}}
{{include "rend3-routine/math/color.wgsl"}}
{{include "rend3-routine/shadow/pcf.wgsl"}}
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
var shadows: texture_depth_2d_array;

@group(1) @binding(0)
var<storage> object_buffer: array<Object>;
@group(1) @binding(0)
var<storage> batch_data: BatchData;

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

var<storage> vertex_buffer: array<u32>;

fn extract_attribute_vec2_f32(byte_base_offset: u32, vertex_index: u32) -> vec2<f32> {
    let first_element_idx = byte_base_offset / 4 + vertex_index * 2;
    return vec2<f32>(
        bitcast<f32>(vertex_buffer[first_element_idx]),
        bitcast<f32>(vertex_buffer[first_element_idx + 1]),
    );
}

fn extract_attribute_vec3_f32(byte_base_offset: u32, vertex_index: u32) -> vec3<f32> {
    let first_element_idx = byte_base_offset / 4 + vertex_index * 3;
    return vec3<f32>(
        bitcast<f32>(vertex_buffer[first_element_idx]),
        bitcast<f32>(vertex_buffer[first_element_idx + 1]),
        bitcast<f32>(vertex_buffer[first_element_idx + 2]),
    );
}

fn extract_attribute_vec4_f32(byte_base_offset: u32, vertex_index: u32) -> vec4<f32> {
    let first_element_idx = byte_base_offset / 4 + vertex_index * 4;
    return vec4<f32>(
        bitcast<f32>(vertex_buffer[first_element_idx]),
        bitcast<f32>(vertex_buffer[first_element_idx + 1]),
        bitcast<f32>(vertex_buffer[first_element_idx + 2]),
        bitcast<f32>(vertex_buffer[first_element_idx + 3]),
    );
}

fn extract_attribute_vec4_u16(byte_base_offset: u32, vertex_index: u32) -> vec4<u32> {
    let first_element_idx = byte_base_offset / 4 + vertex_index * 2;
    let value_0 = vertex_buffer[first_element_idx];
    let value_1 = vertex_buffer[first_element_idx + 1];
    return vec4<u32>(
        value_0 & 0xFFFFu,
        (value_0 >> 16) & 0xFFFFu,
        value_1 & 0xFFFFu,
        (value_1 >> 16) & 0xFFFFu,
    );
}

struct VertexInput {
    position: vec3<f32>,
    normal: vec3<f32>,
    tangent: vec3<f32>,
    coords0: vec2<f32>,
    coords1: vec2<f32>,
    color: vec4<u32>,
}

struct Indices {
    vertex: u32,
    object: u32,
}

fn unpack_vertex_index(vertex_index: u32) -> Indices {
    let local_object_id = vertex_index >> 24;
    let vertex_id = vertex_index & 0xFFFFFFu;
    let object_id = batch_data.ranges[local_object_id].object_id;

    return Indices(vertex_id, object_id);
}

fn get_vertices(indices: Indices) -> VertexInput {
    var verts: VertexInput;
    verts.position = extract_attribute_vec3_f32(object_buffer[indices.object].vertex_attribute_start_offsets[0], indices.vertex);
    verts.normal = extract_attribute_vec3_f32(object_buffer[indices.object].vertex_attribute_start_offsets[1], indices.vertex);
    verts.tangent = extract_attribute_vec3_f32(object_buffer[indices.object].vertex_attribute_start_offsets[2], indices.vertex);
    verts.coords0 = extract_attribute_vec2_f32(object_buffer[indices.object].vertex_attribute_start_offsets[3], indices.vertex);
    verts.coords1 = extract_attribute_vec2_f32(object_buffer[indices.object].vertex_attribute_start_offsets[4], indices.vertex);
    verts.color = extract_attribute_vec4_u16(object_buffer[indices.object].vertex_attribute_start_offsets[5], indices.vertex);
    return verts;
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


@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let indices = unpack_vertex_index(vertex_index);

    let vs_in = get_vertices(indices);
    let data = object_buffer[vs_in.object_idx];

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

fn compute_diffuse_color(base_color: vec3<f32>, metallic: f32) -> vec3<f32> {
    return base_color * (1.0 - metallic);
}

fn compute_f0(base_color: vec3<f32>, metallic: f32, reflectance: f32) -> vec3<f32> {
    return base_color * metallic + (reflectance * (1.0 - metallic));
}

fn compute_dielectric_f0(reflectance: f32) -> f32 {
    return 0.16 * reflectance * reflectance;
}

fn perceptual_roughness_to_roughness(perceptual_roughness: f32) -> f32 {
    return perceptual_roughness * perceptual_roughness;
}

fn get_pixel_data_inner(material_arg: Material, s: sampler, vs_out: VertexOutput) -> PixelData {
    var material = material_arg;
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
    } else if (extract_material_flag(material.flags, FLAGS_AOMR_BW_SPLIT)) {
        // In ao texture:
        // Red: AO
        // In metallic texture:
        // Red: Metallic
        // In roughness texture:
        // Red: Roughness
        if (has_roughness_texture(&material)) {
            pixel.perceptual_roughness = material.roughness * roughness_texture(&material, s, coords, uvdx, uvdy).r;
        } else {
            pixel.perceptual_roughness = material.roughness;
        }

        if (has_metallic_texture(&material)) {
            pixel.metallic = material.metallic * metallic_texture(&material, s, coords, uvdx, uvdy).r;
        } else {
            pixel.metallic = material.metallic;
        }

        if (has_ambient_occlusion_texture(&material)) {
            pixel.ambient_occlusion = material.ambient_occlusion * ambient_occlusion_texture(&material, s, coords, uvdx, uvdy).r;
        } else {
            pixel.ambient_occlusion = material.ambient_occlusion;
        }
    } else {
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

    // --- REFLECTANCE ---

    if (has_reflectance_texture(&material)) {
        pixel.reflectance = material.reflectance * reflectance_texture(&material, s, coords, uvdx, uvdy).r;
    } else {
        pixel.reflectance = material.reflectance;
    }

    // --- CLEARCOAT ---

    if (extract_material_flag(material.flags, FLAGS_CC_GLTF_COMBINED)) {
        if (has_clear_coat_texture(&material)) {
            let texture_read = clear_coat_texture(&material, s, coords, uvdx, uvdy);
            pixel.clear_coat = material.clear_coat * texture_read.r;
            pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness * texture_read.g;
        } else {
            pixel.clear_coat = material.clear_coat;
            pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness;
        }
    } else {
        if (has_clear_coat_texture(&material)) {
            pixel.clear_coat = material.clear_coat * clear_coat_texture(&material, s, coords, uvdx, uvdy).r;
        } else {
            pixel.clear_coat = material.clear_coat;
        }

        if (has_clear_coat_roughness_texture(&material)) {
            let texture_read = clear_coat_roughness_texture(&material, s, coords, uvdx, uvdy);

            if (extract_material_flag(material.flags, FLAGS_CC_GLTF_SPLIT)) {
                pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness * texture_read.g;
            } else {
                pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness * texture_read.r;
            }
        } else {
            pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness;
        }
    }

    // --- EMISSIVE ---

    if (has_emissive_texture(&material)) {
        pixel.emissive = material.emissive * emissive_texture(&material, s, coords, uvdx, uvdy).rgb;
    } else {
        pixel.emissive = material.emissive;
    }

    // --- ANISOTROPY ---

    if (has_anisotropy_texture(&material)) {
        pixel.anisotropy = material.anisotropy * anisotropy_texture(&material, s, coords, uvdx, uvdy).r;
    } else {
        pixel.anisotropy = material.anisotropy;
    }

    // --- COMPUTATIONS---

    pixel.diffuse_color = compute_diffuse_color(pixel.albedo.xyz, pixel.metallic);

    // Assumes an interface from air to an IOR of 1.5 for dielectrics
    let reflectance = compute_dielectric_f0(pixel.reflectance);
    pixel.f0 = compute_f0(pixel.albedo.rgb, pixel.metallic, reflectance);

    if (pixel.clear_coat != 0.0) {
        let base_perceptual_roughness = max(pixel.perceptual_roughness, pixel.clear_coat_perceptual_roughness);
        pixel.perceptual_roughness = mix(pixel.perceptual_roughness, base_perceptual_roughness, pixel.clear_coat);
        pixel.clear_coat_roughness = perceptual_roughness_to_roughness(pixel.clear_coat_perceptual_roughness);
    }
    pixel.roughness = perceptual_roughness_to_roughness(pixel.perceptual_roughness);

    return pixel;
}

{{#if (eq profile "GpuDriven")}}
fn get_pixel_data(material: Material, vs_out: VertexOutput) -> PixelData {
    if (extract_material_flag(material.flags, FLAGS_NEAREST)) {
        return get_pixel_data_inner(material, nearest_sampler, vs_out);
    } else {
        return get_pixel_data_inner(material, primary_sampler, vs_out);
    }
}
{{else}}
fn get_pixel_data(material: Material, vs_out: VertexOutput) -> PixelData {
    return get_pixel_data_inner(material, primary_sampler, vs_out);
}
{{/if}}

fn surface_shading(light: DirectionalLight, pixel: PixelData, view_pos: vec3<f32>, occlusion: f32) -> vec3<f32> {
    let view_mat3 = mat3x3<f32>(uniforms.view[0].xyz, uniforms.view[1].xyz, uniforms.view[2].xyz);
    let l = normalize(view_mat3 * -light.direction);

    let n = pixel.normal;
    let h = normalize(view_pos + l);

    let nov = abs(dot(n, view_pos)) + 0.00001;
    let nol = saturate(dot(n, l));
    let noh = saturate(dot(n, h));
    let loh = saturate(dot(l, h));

    let f90 = saturate(dot(pixel.f0, vec3<f32>(50.0 * 0.33)));

    let d = brdf_d_ggx(noh, pixel.roughness);
    let f = brdf_f_schlick_vec3(loh, pixel.f0, f90);
    let v = brdf_v_smith_ggx_correlated(nov, nol, pixel.roughness);

    // TODO: figure out how they generate their lut
    let energy_comp = 1.0;

    // specular
    let fr = (d * v) * f;
    // diffuse
    let fd = pixel.diffuse_color * brdf_fd_lambert();

    let color = fd + fr * energy_comp;

    let light_attenuation = 1.0;

    return (color * light.color) * (light_attenuation * nol * occlusion);
}

@fragment
fn fs_main(vs_out: VertexOutput) -> @location(0) vec4<f32> {
    {{#if (eq profile "GpuDriven")}}
    let material = materials[vs_out.material];
    {{/if}}

    let pixel = get_pixel_data(material, vs_out);

    if (extract_material_flag(material.flags, FLAGS_UNLIT)) {
        return pixel.albedo;
    }

    let v = -normalize(vs_out.view_position.xyz);

    var color = pixel.emissive.rgb;
    for (var i = 0; i < i32(directional_lights.count); i += 1) {
        let light = directional_lights.data[i];

        let shadow_ndc = (light.view_proj * uniforms.inv_view * vs_out.view_position).xyz;
        let shadow_flipped = (shadow_ndc.xy * 0.5) + 0.5;
        let shadow_coords = vec2<f32>(shadow_flipped.x, 1.0 - shadow_flipped.y);

        var shadow_value = 1.0;
        if (any(shadow_flipped >= vec2<f32>(0.0)) || any(shadow_flipped <= vec2<f32>(1.0))) {
            shadow_value = shadow_sample_pcf5(shadows, comparison_sampler, shadow_coords, i, shadow_ndc.z);
        }

        color += surface_shading(light, pixel, v, shadow_value * pixel.ambient_occlusion);
    }

    let ambient = uniforms.ambient * pixel.albedo;
    let shaded = vec4<f32>(color, pixel.albedo.a);
    return max(ambient, shaded);
}
