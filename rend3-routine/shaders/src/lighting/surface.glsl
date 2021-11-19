// These are derived by the wonderful https://google.github.io/filament/Filament.md.html

#ifndef SHADER_LIGHTING_SURFACE_GLSL
#define SHADER_LIGHTING_SURFACE_GLSL

#include "brdf.glsl"
#include "pixel.glsl"

vec3 surface_shading(DirectionalLight light, PixelData pixel, vec3 v, float occlusion) {
    vec3 l = normalize(mat3(uniforms.view) * -light.direction);

    vec3 n = pixel.normal;
    vec3 h = normalize(v + l);

    float NoV = abs(dot(n, v)) + 1e-5;
    float NoL = saturate(dot(n, l));
    float NoH = saturate(dot(n, h));
    float LoH = saturate(dot(l, h));

    float f90 = saturate(dot(pixel.f0, vec3(50.0 * 0.33)));

    float D = D_GGX(NoH, pixel.roughness);
    vec3  F = F_Schlick(LoH, pixel.f0, f90);
    float V = V_SmithGGXCorrelated(NoV, NoL, pixel.roughness);

    // TODO: figure out how they generate their lut
    float energy_compensation = 1.0;

    // specular
    vec3 Fr = (D * V) * F;

    // diffuse
    vec3 Fd = pixel.diffuse_color * Fd_Lambert();

    vec3 color = Fd + Fr * energy_compensation;

    float light_attenuation = 1.0;

    return (color * light.color) * (light_attenuation * NoL * occlusion);
}

#endif
