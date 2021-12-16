struct CPUMaterialData {
    uv_transform0_: mat3x3<f32>;
    uv_transform1_: mat3x3<f32>;
    albedo: vec4<f32>;
    emissive: vec3<f32>;
    roughness: f32;
    metallic: f32;
    reflectance: f32;
    clear_coat: f32;
    clear_coat_roughness: f32;
    anisotropy: f32;
    ambient_occlusion: f32;
    alpha_cutout: f32;
    material_flags: u32;
    texture_enable: u32;
};

struct TextureData {
    material: CPUMaterialData;
};

[[group(2), binding(10)]]
var<uniform> unnamed: TextureData;
var<private> i_coords0_1: vec2<f32>;
[[group(2), binding(0)]]
var albedo_tex: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> i_position_1: vec4<f32>;
var<private> i_color_1: vec4<f32>;
var<private> i_material_1: u32;

fn main_1() {
    let _e19 = unnamed.material.texture_enable;
    let _e28 = unnamed.material.uv_transform0_;
    let _e29 = i_coords0_1;
    let _e33 = (_e28 * vec3<f32>(_e29.x, _e29.y, 1.0));
    let _e36 = vec2<f32>(_e33.x, _e33.y);
    let _e37 = dpdx(_e36);
    let _e38 = dpdy(_e36);
    if ((bitcast<bool>(((_e19 >> bitcast<u32>(0)) & 1u)) != bitcast<bool>(0u))) {
        let _e39 = textureSampleGrad(albedo_tex, primary_sampler, _e36, _e37, _e38);
        let _e43 = unnamed.material.alpha_cutout;
        if ((_e39.w <= _e43)) {
            discard;
        }
    }
    return;
}

[[stage(fragment)]]
fn main([[location(1)]] i_coords0_: vec2<f32>, [[location(0)]] i_position: vec4<f32>, [[location(2)]] i_color: vec4<f32>, [[location(3)]] i_material: u32) {
    i_coords0_1 = i_coords0_;
    i_position_1 = i_position;
    i_color_1 = i_color;
    i_material_1 = i_material;
    main_1();
}
