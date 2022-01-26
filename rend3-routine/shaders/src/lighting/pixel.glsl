#ifndef SHADER_LIGHTING_PIXEL_GLSL
#define SHADER_LIGHTING_PIXEL_GLSL

#include "texture_access.glsl"
#include "../structures.glsl"
#include "../math.glsl"

vec3 compute_diffuse_color(const vec4 baseColor, float metallic) {
    return baseColor.rgb * (1.0 - metallic);
}

vec3 compute_f0(const vec4 baseColor, float metallic, float reflectance) {
    return baseColor.rgb * metallic + (reflectance * (1.0 - metallic));
}

float compute_dielectric_f0(float reflectance) {
    return 0.16 * reflectance * reflectance;
}

float perceptual_roughness_to_roughness(float perceptual_roughness) {
    return perceptual_roughness * perceptual_roughness;
}

struct PixelData {
    vec4 albedo;
    vec3 diffuse_color;
    float roughness;
    vec3 normal;
    float metallic;
    vec3 f0;
    float perceptual_roughness;
    vec3 emissive;
    float reflectance;
    float clear_coat;
    float clear_coat_roughness;
    float clear_coat_perceptual_roughness;
    float anisotropy;
    float ambient_occlusion;
    uint material_flags;
};

PixelData get_per_pixel_data_sampled(MATERIAL_TYPE material, sampler s) {
    PixelData pixel;
    
    vec2 coords = vec2(material.uv_transform0 * vec3(i_coords0, 1.0));
    vec2 uvdx = dFdx(coords);
    vec2 uvdy = dFdy(coords);

    if (MATERIAL_FLAG(FLAGS_ALBEDO_ACTIVE)) {
        if (HAS_ALBEDO_TEXTURE) {
            pixel.albedo = textureGrad(sampler2D(ALBEDO_TEXTURE, s), coords, uvdx, uvdy);
        } else {
            pixel.albedo = vec4(1.0);
        }
        if (MATERIAL_FLAG(FLAGS_ALBEDO_BLEND)) {
            vec4 vert_color = i_color;
            if (MATERIAL_FLAG(FLAGS_ALBEDO_VERTEX_SRGB)) {
                vert_color = srgb_to_linear(vert_color);
            }
            pixel.albedo *= vert_color;
        }
    } else {
        pixel.albedo = vec4(0.0, 0.0, 0.0, 1.0);
    }
    pixel.albedo *= material.albedo;

    if (MATERIAL_FLAG(FLAGS_UNLIT)) {
        pixel.normal = normalize(i_normal);
    }
    else {
        if (HAS_NORMAL_TEXTURE) {
            vec4 texture_read = textureGrad(sampler2D(NORMAL_TEXTURE, s), coords, uvdx, uvdy);
            vec3 normal;
            if (MATERIAL_FLAG(FLAGS_BICOMPONENT_NORMAL)) {
                vec2 bicomp;
                if (MATERIAL_FLAG(FLAGS_SWIZZLED_NORMAL)) {
                    bicomp = texture_read.ag;
                } else {
                    bicomp = texture_read.rg;
                }
                bicomp = bicomp * 2.0 - 1.0;

                normal = vec3(bicomp, sqrt(1 - (bicomp.r * bicomp.r) - (bicomp.g * bicomp.g)));
            } else {
                normal = normalize(texture_read.rgb * 2.0 - 1.0);
            }
            if (MATERIAL_FLAG(FLAGS_YDOWN_NORMAL)) {
                normal.y = -normal.y;
            }
            vec3 normal_norm = normalize(i_normal);
            vec3 tangent_norm = normalize(i_tangent);
            vec3 bitangent = cross(normal_norm, tangent_norm);

            mat3 tbn = mat3(tangent_norm, bitangent, normal_norm);

            pixel.normal = tbn * normal;
        } else {
            pixel.normal = i_normal;
        }
        pixel.normal = normalize(pixel.normal);

        // Extract AO, metallic, and roughness data from various packed formats

        // In roughness texture:
        // Red: AO
        // Green: Roughness
        // Blue: Metallic
        if (MATERIAL_FLAG(FLAGS_AOMR_COMBINED)) {
            if (HAS_ROUGHNESS_TEXTURE) {
                vec3 aomr = textureGrad(sampler2D(ROUGHNESS_TEXTURE, s), coords, uvdx, uvdy).rgb;
                pixel.ambient_occlusion = material.ambient_occlusion * aomr[0];
                pixel.perceptual_roughness = material.roughness * aomr[1];
                pixel.metallic = material.metallic * aomr[2];
            } else {
                pixel.ambient_occlusion = material.ambient_occlusion;
                pixel.metallic = material.metallic;
                pixel.perceptual_roughness = material.roughness;
            }
        }
        // In ao texture:
        // Red: AO
        // In roughness texture:
        // Green: Roughness
        // Blue: Metallic
        else if (MATERIAL_FLAG(FLAGS_AOMR_SWIZZLED_SPLIT) || MATERIAL_FLAG(FLAGS_AOMR_SPLIT)) {
            if (HAS_ROUGHNESS_TEXTURE) {
                vec4 texture_read = textureGrad(sampler2D(ROUGHNESS_TEXTURE, s), coords, uvdx, uvdy);
                vec2 mr = MATERIAL_FLAG(FLAGS_AOMR_SWIZZLED_SPLIT) ? texture_read.gb : texture_read.rg;
                pixel.perceptual_roughness = material.roughness * mr[0];
                pixel.metallic = material.metallic * mr[1];
            } else {
                pixel.metallic = material.metallic;
                pixel.perceptual_roughness = material.roughness;
            }
            if (HAS_AMBIENT_OCCLUSION_TEXTURE) {
                pixel.ambient_occlusion = material.ambient_occlusion * textureGrad(sampler2D(AMBIENT_OCCLUSION_TEXTURE, s), coords, uvdx, uvdy).r;
            } else {
                pixel.ambient_occlusion = material.ambient_occlusion;
            }
        }
        // In ao texture:
        // Red: AO
        // In metallic texture:
        // Red: Metallic
        // In roughness texture:
        // Red: Roughness
        else if (MATERIAL_FLAG(FLAGS_AOMR_BW_SPLIT)) {
            if (HAS_ROUGHNESS_TEXTURE) {
                pixel.perceptual_roughness = material.roughness * textureGrad(sampler2D(ROUGHNESS_TEXTURE, s), coords, uvdx, uvdy).r;
            } else {
                pixel.perceptual_roughness = material.roughness;
            }

            if (HAS_METALLIC_TEXTURE) {
                pixel.metallic = material.metallic * textureGrad(sampler2D(METALLIC_TEXTURE, s), coords, uvdx, uvdy).r;
            } else {
                pixel.metallic = material.metallic;
            }

            if (HAS_AMBIENT_OCCLUSION_TEXTURE) {
                pixel.ambient_occlusion = material.ambient_occlusion * textureGrad(sampler2D(AMBIENT_OCCLUSION_TEXTURE, s), coords, uvdx, uvdy).r;
            } else {
                pixel.ambient_occlusion = material.ambient_occlusion;
            }
        }

        if (HAS_REFLECTANCE_TEXTURE) {
            pixel.reflectance = material.reflectance * textureGrad(sampler2D(REFLECTANCE_TEXTURE, s), coords, uvdx, uvdy).r;
        } else {
            pixel.reflectance = material.reflectance;
        }

        pixel.diffuse_color = compute_diffuse_color(pixel.albedo, pixel.metallic);
        // Assumes an interface from air to an IOR of 1.5 for dielectrics
        float reflectance = compute_dielectric_f0(pixel.reflectance);
        pixel.f0 = compute_f0(pixel.albedo, pixel.metallic, reflectance);

        if (MATERIAL_FLAG(FLAGS_CC_GLTF_COMBINED)) {
            if (HAS_CLEAR_COAT_TEXTURE) {
                vec2 cc = textureGrad(sampler2D(CLEAR_COAT_TEXTURE, s), coords, uvdx, uvdy).rg;
                pixel.clear_coat = material.clear_coat * cc.r;
                pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness * cc.g;
            } else {
                pixel.clear_coat = material.clear_coat;
                pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness;
            }
        } else if (MATERIAL_FLAG(FLAGS_CC_GLTF_SPLIT)) {
            if (HAS_CLEAR_COAT_TEXTURE) {
                pixel.clear_coat = material.clear_coat * textureGrad(sampler2D(CLEAR_COAT_TEXTURE, s), coords, uvdx, uvdy).r;
            } else {
                pixel.clear_coat = material.clear_coat;
            }
            if (HAS_CLEAR_COAT_ROUGHNESS_TEXTURE) {
                pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness * textureGrad(sampler2D(CLEAR_COAT_ROUGHNESS_TEXTURE, s), coords, uvdx, uvdy).g;
            } else {
                pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness;
            }
        } else if (MATERIAL_FLAG(FLAGS_CC_BW_SPLIT)) {
            if (HAS_CLEAR_COAT_TEXTURE) {
                pixel.clear_coat = material.clear_coat * textureGrad(sampler2D(CLEAR_COAT_TEXTURE, s), coords, uvdx, uvdy).r;
            } else {
                pixel.clear_coat = material.clear_coat;
            }
            if (HAS_CLEAR_COAT_ROUGHNESS_TEXTURE) {
                pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness * textureGrad(sampler2D(CLEAR_COAT_ROUGHNESS_TEXTURE, s), coords, uvdx, uvdy).r;
            } else {
                pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness;
            }
        }

        if (pixel.clear_coat != 0.0) {
            float base_perceptual_roughness = max(pixel.perceptual_roughness, pixel.clear_coat_perceptual_roughness);
            pixel.perceptual_roughness = mix(pixel.perceptual_roughness, base_perceptual_roughness, pixel.clear_coat);

            pixel.clear_coat_roughness = perceptual_roughness_to_roughness(pixel.clear_coat_perceptual_roughness);
        }
        pixel.roughness = perceptual_roughness_to_roughness(pixel.perceptual_roughness);

        if (HAS_EMISSIVE_TEXTURE) {
            pixel.emissive = material.emissive * textureGrad(sampler2D(EMISSIVE_TEXTURE, s), coords, uvdx, uvdy).rgb;
        } else {
            pixel.emissive = material.emissive;
        }

        // TODO: Aniso info
        if (HAS_ANISOTROPY_TEXTURE) {
            pixel.anisotropy = material.anisotropy * textureGrad(sampler2D(ANISOTROPY_TEXTURE, s), coords, uvdx, uvdy).r;
        } else {
            pixel.anisotropy = material.anisotropy;
        }
    }

    pixel.material_flags = material.material_flags;

    return pixel;
}

#ifdef GPU_DRIVEN
PixelData get_per_pixel_data(MATERIAL_TYPE material) {
    if (MATERIAL_FLAG(FLAGS_NEAREST)) {
        return get_per_pixel_data_sampled(material, nearest_sampler);
    } else {
        return get_per_pixel_data_sampled(material, primary_sampler);
    }
}
#endif

#ifdef CPU_DRIVEN
// In the CpuDriven profile the primary sampler gets switched out for what we need, so we don't switch in the shader.
// This is because OpenGL can't deal with any texture being used with multiple different samplers. 
PixelData get_per_pixel_data(MATERIAL_TYPE material) {
    return get_per_pixel_data_sampled(material, primary_sampler);
}
#endif

#endif
