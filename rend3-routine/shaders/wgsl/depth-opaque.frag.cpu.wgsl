var<private> i_position1: vec4<f32>;
var<private> i_coords0_1: vec2<f32>;
var<private> i_color1: vec4<f32>;
var<private> i_material1: u32;

fn main1() {
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] i_position: vec4<f32>, [[location(1)]] i_coords0: vec2<f32>, [[location(2)]] i_color: vec4<f32>, [[location(3)]] i_material: u32) {
    i_position1 = i_position;
    i_coords0_1 = i_coords0;
    i_color1 = i_color;
    i_material1 = i_material;
    main1();
}
