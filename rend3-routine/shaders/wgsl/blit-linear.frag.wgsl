[[group(1), binding(0)]]
var source: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> tex_coords_1: vec2<f32>;
var<private> color: vec4<f32>;

fn main_1() {
    let _e8 = tex_coords_1;
    let _e9 = textureSample(source, primary_sampler, _e8);
    color = _e9;
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] tex_coords: vec2<f32>) -> [[location(0)]] vec4<f32> {
    tex_coords_1 = tex_coords;
    main_1();
    let _e3 = color;
    return _e3;
}
