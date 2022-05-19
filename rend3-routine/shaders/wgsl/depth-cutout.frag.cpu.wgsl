struct DataAbi {
    stride: u32,
    texture_offset: u32,
    cutoff_offset: u32,
    uv_transform_offset: u32,
}

struct TextureData {
    material_data: array<f32>,
}

@group(2) @binding(0) 
var<uniform> unnamed: DataAbi;
var<private> i_material_1: u32;
@group(3) @binding(0) 
var<storage> unnamed_1: TextureData;
var<private> i_coords0_1: vec2<f32>;
@group(3) @binding(1) 
var texture_: texture_2d<f32>;
@group(0) @binding(0) 
var primary_sampler: sampler;

fn main_1() {
    var phi_168_: vec2<f32>;

    let _e26 = unnamed.stride;
    let _e27 = i_material_1;
    let _e28 = (_e26 * _e27);
    let _e30 = unnamed.cutoff_offset;
    let _e34 = unnamed_1.material_data[(_e28 + _e30)];
    let _e36 = unnamed.uv_transform_offset;
    if (_e36 != 4294967295u) {
        let _e38 = (_e28 + _e36);
        let _e41 = unnamed_1.material_data[_e38];
        let _e45 = unnamed_1.material_data[(_e38 + 1u)];
        let _e49 = unnamed_1.material_data[(_e38 + 2u)];
        let _e53 = unnamed_1.material_data[(_e38 + 4u)];
        let _e57 = unnamed_1.material_data[(_e38 + 5u)];
        let _e61 = unnamed_1.material_data[(_e38 + 6u)];
        let _e65 = unnamed_1.material_data[(_e38 + 8u)];
        let _e69 = unnamed_1.material_data[(_e38 + 9u)];
        let _e73 = unnamed_1.material_data[(_e38 + 10u)];
        let _e78 = i_coords0_1;
        let _e82 = (mat3x3<f32>(vec3<f32>(_e41, _e45, _e49), vec3<f32>(_e53, _e57, _e61), vec3<f32>(_e65, _e69, _e73)) * vec3<f32>(_e78.x, _e78.y, 1.0));
        phi_168_ = vec2<f32>(_e82.x, _e82.y);
    } else {
        let _e86 = i_coords0_1;
        phi_168_ = _e86;
    }
    let _e88 = phi_168_;
    let _e89 = dpdx(_e88);
    let _e90 = dpdy(_e88);
    let _e92 = unnamed.texture_offset;
    let _e96 = unnamed_1.material_data[(_e28 + _e92)];
    if ((bitcast<u32>(_e96) & 1u) != 0u) {
        let _e100 = textureSampleGrad(texture_, primary_sampler, _e88, _e89, _e90);
        if (_e100.w <= _e34) {
            discard;
        }
    }
    return;
}

@fragment 
fn main(@location(3) i_material: u32, @location(1) i_coords0_: vec2<f32>) {
    i_material_1 = i_material;
    i_coords0_1 = i_coords0_;
    main_1();
}
