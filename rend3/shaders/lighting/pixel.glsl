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
    float reflectance;
    float clear_coat;
    float clear_coat_roughness;
    float clear_coat_perceptual_roughness;
    float anisotropy;
    float ambient_occlusion;
    float alpha_cutout;
    uint material_flags;
};

PixelData get_per_pixel_data(MATERIAL_TYPE material) {
    PixelData pixel;

    if (bool(material.material_flags & FLAGS_ALBEDO_ACTIVE)) {
        if (HAS_ALBEDO_TEXTURE) {
            pixel.albedo = texture(sampler2D(ALBEDO_TEXTURE, linear_sampler), i_coords);
        } else {
            pixel.albedo = material.albedo;
        }
        if (bool(material.material_flags & FLAGS_ALBEDO_BLEND)) {
            vec4 vert_color = i_color;
            if (bool(material.material_flags & FLAGS_ALBEDO_VERTEX_SRGB)) {
                vert_color = srgb_to_linear(vert_color);
            }
            pixel.albedo *= vert_color;
        }
    } else {
        pixel.albedo = vec4(0.0, 0.0, 0.0, 1.0);
    }

    if (HAS_NORMAL_TEXTURE) {
        // TODO: normal mapping
        pixel.normal = i_normal;
    } else {
        pixel.normal = i_normal;
    }
    pixel.normal = normalize(pixel.normal);

    if (HAS_ROUGHNESS_TEXTURE) {
        pixel.perceptual_roughness = texture(sampler2D(ROUGHNESS_TEXTURE, linear_sampler), i_coords).r;
    } else {
        pixel.perceptual_roughness = material.roughness;
    }

    if (HAS_METALLIC_TEXTURE) {
        pixel.metallic = texture(sampler2D(METALLIC_TEXTURE, linear_sampler), i_coords).r;
    } else {
        pixel.metallic = material.metallic;
    }

    if (HAS_REFLECTANCE_TEXTURE) {
        pixel.reflectance = texture(sampler2D(REFLECTANCE_TEXTURE, linear_sampler), i_coords).r;
    } else {
        pixel.reflectance = material.reflectance;
    }

    pixel.diffuse_color = compute_diffuse_color(pixel.albedo, pixel.metallic);
    // Assumes an interface from air to an IOR of 1.5 for dielectrics
    float reflectance = compute_dielectric_f0(pixel.reflectance);
    pixel.f0 = compute_f0(pixel.albedo, pixel.metallic, reflectance);

    if (HAS_CLEAR_COAT_TEXTURE) {
        pixel.clear_coat = texture(sampler2D(CLEAR_COAT_TEXTURE, linear_sampler), i_coords).r;
    } else {
        pixel.clear_coat = material.clear_coat;
    }
    if (pixel.clear_coat != 0.0) {
        if (HAS_CLEAR_COAT_ROUGHNESS_TEXTURE) {
            pixel.clear_coat_perceptual_roughness = texture(sampler2D(CLEAR_COAT_ROUGHNESS_TEXTURE, linear_sampler), i_coords).r;
        } else {
            pixel.clear_coat_perceptual_roughness = material.clear_coat_roughness;
        }

        float base_perceptual_roughness = max(pixel.perceptual_roughness, pixel.clear_coat_perceptual_roughness);
        pixel.perceptual_roughness = mix(pixel.perceptual_roughness, base_perceptual_roughness, pixel.clear_coat);

        pixel.clear_coat_roughness = perceptual_roughness_to_roughness(pixel.clear_coat_perceptual_roughness);
    }
    pixel.roughness = perceptual_roughness_to_roughness(pixel.perceptual_roughness);

    // TODO: Aniso info
    if (HAS_ANISOTROPY_TEXTURE) {
        pixel.anisotropy = texture(sampler2D(ANISOTROPY_TEXTURE, linear_sampler), i_coords).r;
    } else {
        pixel.anisotropy = material.anisotropy;
    }

    if (HAS_AMBIENT_OCCLUSION_TEXTURE) {
        pixel.ambient_occlusion = texture(sampler2D(AMBIENT_OCCLUSION_TEXTURE, linear_sampler), i_coords).r;
    } else {
        pixel.ambient_occlusion = material.ambient_occlusion;
    }

    pixel.alpha_cutout = material.alpha_cutout;
    pixel.material_flags = material.material_flags;

    return pixel;
}

#endif
