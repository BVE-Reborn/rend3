[[group(1), binding(0)]]
var source: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> tex_coords_1: vec2<f32>;
var<private> color: vec4<f32>;

fn main_1() {
    let _e20 = tex_coords_1;
    let _e21 = textureSample(source, primary_sampler, _e20);
    let _e22 = _e21.xyz;
    let _e30 = mix((_e22 * 12.920000076293945), ((pow(_e22, vec3<f32>(0.41666001081466675, 0.41666001081466675, 0.41666001081466675)) * 1.0549999475479126) - vec3<f32>(0.054999999701976776, 0.054999999701976776, 0.054999999701976776)), clamp(ceil((_e22 - vec3<f32>(0.0031308000907301903, 0.0031308000907301903, 0.0031308000907301903))), vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0)));
    color = vec4<f32>(_e30.x, _e30.y, _e30.z, _e21.w);
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] tex_coords: vec2<f32>) -> [[location(0)]] vec4<f32> {
    tex_coords_1 = tex_coords;
    main_1();
    let _e3 = color;
    return _e3;
}
