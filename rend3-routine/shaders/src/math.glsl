
#ifndef SHADER_MATH_GLSL
#define SHADER_MATH_GLSL

#define PI       3.14159265359
#define HALF_PI  1.570796327

float saturate(float x) {
    return clamp(x, 0.0, 1.0);
}

vec4 srgb_to_linear(vec4 srgb) {
    vec3 color_srgb = srgb.rgb;
    vec3 selector = clamp(ceil(color_srgb - 0.04045), 0.0, 1.0); // 0 if under value, 1 if over
    vec3 under = color_srgb / 12.92;
    vec3 over = pow((color_srgb + 0.055) / 1.055, vec3(2.4));
    vec3 result = mix(under, over, selector);
    return vec4(result, srgb.a);
}

vec4 linear_to_srgb(vec4 linear) {
    vec3 color_linear = linear.rgb;
    vec3 selector = clamp(ceil(color_linear - 0.0031308), 0.0, 1.0); // 0 if under value, 1 if over
    vec3 under = 12.92 * color_linear;
    vec3 over = 1.055 * pow(color_linear, vec3(0.41666)) - 0.055;
    vec3 result = mix(under, over, selector);
    return vec4(result, linear.a);
}

#endif