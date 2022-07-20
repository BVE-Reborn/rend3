// The SRGB EOTF
// aka "srgb_to_linear" but with a better name
fn srgb_display_to_scene(electro: vec3<f32>) -> vec3<f32> {
    let selector = electro > vec3<f32>(0.04045);
    let under = electro / 12.92;
    let over = pow((electro + 0.055) / 1.055, vec3<f32>(2.4));
    let optical = select(under, over, selector);
    return optical;
}

// The SRGB OETF
// aka "linear_to_srgb" but with a better name
fn srgb_scene_to_display(opto: vec3<f32>) -> vec3<f32> {
    let selector = opto > vec3<f32>(0.0031308);
    let under = opto * 12.92;
    let over = 1.055 * pow(opto, vec3<f32>(0.4166)) - 0.055;
    let electrical = select(under, over, selector);
    return electrical;
}

fn saturate(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}
