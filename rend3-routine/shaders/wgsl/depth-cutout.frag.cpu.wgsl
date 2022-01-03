struct DataAbi {
    stride: u32;
    texture_offset: u32;
    cutoff_offset: u32;
    uv_transform_offset: u32;
};

struct TextureData {
    material_data: [[stride(4)]] array<f32>;
};

[[group(2), binding(0)]]
var<uniform> unnamed: DataAbi;
var<private> i_material_1: u32;
[[group(3), binding(0)]]
var<storage> unnamed_1: TextureData;
var<private> i_coords0_1: vec2<f32>;
[[group(3), binding(1)]]
var texture: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> i_position_1: vec4<f32>;
var<private> i_color_1: vec4<f32>;

fn main_1() {
    var phi_168_: vec2<f32>;

    let _e28 = unnamed.stride;
    let _e29 = i_material_1;
    let _e30 = (_e28 * _e29);
    let _e32 = unnamed.cutoff_offset;
    let _e36 = unnamed_1.material_data[(_e30 + _e32)];
    let _e38 = unnamed.uv_transform_offset;
    if ((_e38 != 4294967295u)) {
        let _e40 = (_e30 + _e38);
        let _e43 = unnamed_1.material_data[_e40];
        let _e47 = unnamed_1.material_data[(_e40 + 1u)];
        let _e51 = unnamed_1.material_data[(_e40 + 2u)];
        let _e55 = unnamed_1.material_data[(_e40 + 4u)];
        let _e59 = unnamed_1.material_data[(_e40 + 5u)];
        let _e63 = unnamed_1.material_data[(_e40 + 6u)];
        let _e67 = unnamed_1.material_data[(_e40 + 8u)];
        let _e71 = unnamed_1.material_data[(_e40 + 9u)];
        let _e75 = unnamed_1.material_data[(_e40 + 10u)];
        let _e80 = i_coords0_1;
        let _e84 = (mat3x3<f32>(vec3<f32>(_e43, _e47, _e51), vec3<f32>(_e55, _e59, _e63), vec3<f32>(_e67, _e71, _e75)) * vec3<f32>(_e80.x, _e80.y, 1.0));
        phi_168_ = vec2<f32>(_e84.x, _e84.y);
    } else {
        let _e88 = i_coords0_1;
        phi_168_ = _e88;
    }
    let _e90 = phi_168_;
    let _e91 = dpdx(_e90);
    let _e92 = dpdy(_e90);
    let _e94 = unnamed.texture_offset;
    let _e98 = unnamed_1.material_data[(_e30 + _e94)];
    if (((bitcast<u32>(_e98) & 1u) != 0u)) {
        let _e102 = textureSampleGrad(texture, primary_sampler, _e90, _e91, _e92);
        if ((_e102.w <= _e36)) {
            discard;
        }
    }
    return;
}

[[stage(fragment)]]
fn main([[location(3)]] i_material: u32, [[location(1)]] i_coords0_: vec2<f32>, [[location(0)]] i_position: vec4<f32>, [[location(2)]] i_color: vec4<f32>) {
    i_material_1 = i_material;
    i_coords0_1 = i_coords0_;
    i_position_1 = i_position;
    i_color_1 = i_color;
    main_1();
}
