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
    let _e26 = unnamed.material.uv_transform0_;
    let _e27 = i_coords0_1;
    let _e31 = (_e26 * vec3<f32>(_e27.x, _e27.y, 1.0));
    let _e34 = vec2<f32>(_e31.x, _e31.y);
    let _e35 = dpdx(_e34);
    let _e36 = dpdy(_e34);
    if ((((_e19 >> bitcast<u32>(0)) & 1u) != 0u)) {
        let _e37 = textureSampleGrad(albedo_tex, primary_sampler, _e34, _e35, _e36);
        let _e41 = unnamed.material.alpha_cutout;
        if ((_e37.w <= _e41)) {
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
