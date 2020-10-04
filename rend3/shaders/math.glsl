
#ifndef SHADER_MATH_GLSL
#define SHADER_MATH_GLSL

#define PI       3.14159265359
#define HALF_PI  1.570796327

float saturate(float x) {
    return clamp(x, 0.0, 1.0);
}

#endif