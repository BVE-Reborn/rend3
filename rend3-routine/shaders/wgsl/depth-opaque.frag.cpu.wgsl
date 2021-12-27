var<private> i_position_1: vec4<f32>;
var<private> i_coords0_1: vec2<f32>;
var<private> i_color_1: vec4<f32>;
var<private> i_material_1: u32;

fn main_1() {
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] i_position: vec4<f32>, [[location(1)]] i_coords0_: vec2<f32>, [[location(2)]] i_color: vec4<f32>, [[location(3)]] i_material: u32) {
    i_position_1 = i_position;
    i_coords0_1 = i_coords0_;
    i_color_1 = i_color;
    i_material_1 = i_material;
    main_1();
}
