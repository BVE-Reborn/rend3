#ifndef SHADER_LIGHTING_PCF_GLSL
#define SHADER_LIGHTING_PCF_GLSL

float sample_shadow_pcf5(texture2DArray tex, samplerShadow samp, vec4 coords) {
    float result = 0.0f;
    result += textureGrad(sampler2DArrayShadow(tex, samp), coords, vec2(0), vec2(0)) * 0.2;
    result += textureGradOffset(sampler2DArrayShadow(tex, samp), coords, vec2(0), vec2(0), ivec2( 0,  1)) * 0.2;
    result += textureGradOffset(sampler2DArrayShadow(tex, samp), coords, vec2(0), vec2(0), ivec2( 0, -1)) * 0.2;
    result += textureGradOffset(sampler2DArrayShadow(tex, samp), coords, vec2(0), vec2(0), ivec2( 1,  0)) * 0.2;
    result += textureGradOffset(sampler2DArrayShadow(tex, samp), coords, vec2(0), vec2(0), ivec2(-1,  0)) * 0.2;
    return result;
}

#endif
