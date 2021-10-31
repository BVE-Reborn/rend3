struct CPUMaterialData {
    uv_transform0: mat3x3<f32>;
    uv_transform1: mat3x3<f32>;
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

[[block]]
struct TextureData {
    material: CPUMaterialData;
};

[[group(2), binding(10)]]
var<uniform> global: TextureData;
var<private> i_coords0_1: vec2<f32>;
[[group(2), binding(0)]]
var albedo_tex: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> i_position1: vec4<f32>;
var<private> i_color1: vec4<f32>;
var<private> i_material1: u32;

fn main1() {
    let e19: u32 = global.material.texture_enable;
    let e28: mat3x3<f32> = global.material.uv_transform0;
    let e29: vec2<f32> = i_coords0_1;
    let e33: vec3<f32> = (e28 * vec3<f32>(e29.x, e29.y, 1.0));
    let e36: vec2<f32> = vec2<f32>(e33.x, e33.y);
    let e37: vec2<f32> = dpdx(e36);
    let e38: vec2<f32> = dpdy(e36);
    if ((bitcast<i32>(((e19 >> bitcast<u32>(0)) & 1u)) != bitcast<i32>(0u))) {
        let e39: vec4<f32> = textureSampleGrad(albedo_tex, primary_sampler, e36, e37, e38);
        let e43: f32 = global.material.alpha_cutout;
        if ((e39.w <= e43)) {
            discard;
        }
    }
    return;
}

[[stage(fragment)]]
fn main([[location(1)]] i_coords0: vec2<f32>, [[location(0)]] i_position: vec4<f32>, [[location(2)]] i_color: vec4<f32>, [[location(3)]] i_material: u32) {
    i_coords0_1 = i_coords0;
    i_position1 = i_position;
    i_color1 = i_color;
    i_material1 = i_material;
    main1();
}
