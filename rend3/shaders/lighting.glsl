// These are derived by the wonderful https://google.github.io/filament/Filament.md.html

#ifndef SHADER_LIGHTING_GLSL
#define SHADER_LIGHTING_GLSL

#include "math.glsl"

float D_GGX(float NoH, float a) {
    float a2 = a * a;
    float f = (NoH * a2 - NoH) * NoH + 1.0;
    return a2 / (PI * f * f);
}

vec3 F_Schlick(float u, vec3 f0, float f90) {
    return f0 + (f90 - f0) * pow(1.0 - u, 5.0);
}

float F_Schlick(float u, float f0, float f90) {
    return f0 + (f90 - f0) * pow(1.0 - u, 5.0);
}

float Fd_Burley(float NoV, float NoL, float LoH, float roughness) {
    float f90 = 0.5 + 2.0 * roughness * LoH * LoH;
    float lightScatter = F_Schlick(NoL, 1.0, f90);
    float viewScatter = F_Schlick(NoV, 1.0, f90);
    return lightScatter * viewScatter * (1.0 / PI);
}

float V_SmithGGXCorrelated(float NoV, float NoL, float a) {
    float a2 = a * a;
    float GGXL = NoV * sqrt((-NoL * a2 + NoL) * NoL + a2);
    float GGXV = NoL * sqrt((-NoV * a2 + NoV) * NoV + a2);
    return 0.5 / (GGXV + GGXL);
}

vec3 surface_shading(vec3 diffuse_color, vec3 n, vec3 v, vec3 l, vec3 f0, float perceptual_roughness, float occlusion) {
    vec3 h = normalize(v + l);

    float NoV = abs(dot(n, v)) + 1e-5;
    float NoL = saturate(dot(n, l));
    float NoH = saturate(dot(n, h));
    float LoH = saturate(dot(l, h));

    float roughness = perceptual_roughness * perceptual_roughness;

    float f90 = saturate(dot(f0, vec3(50.0 * 0.33)));

    float D = D_GGX(NoH, roughness);
    vec3  F = F_Schlick(LoH, f0, f90);
    float V = V_SmithGGXCorrelated(NoV, NoL, roughness);

    // TODO: figure out how they generate their lut
    float energy_compensation = 1.0;

    // specular
    vec3 Fr = (D * V) * F;

    // diffuse
    vec3 Fd = diffuse_color * Fd_Burley(NoV, NoL, LoH, roughness);

    vec3 color = Fd + Fr * energy_compensation;

    vec4 light_color_intensity = vec4(vec3(10.0), 1.0);
    float light_attenuation = 1.0;

    return (color * light_color_intensity.rgb) * (light_color_intensity.a * light_attenuation * NoL * occlusion);
}

#endif
