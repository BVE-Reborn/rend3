[[group(1), binding(0)]]
var source: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> tex_coords_1: vec2<f32>;
var<private> color: vec4<f32>;

fn main_1() {
    let _e22 = tex_coords_1;
    let _e23 = textureSample(source, primary_sampler, _e22);
    let _e25 = (_e23.xyz * 2.0);
    let _e26 = (_e25 * 0.15000000596046448);
    let _e35 = (((((_e25 * (_e26 + vec3<f32>(0.05000000074505806, 0.05000000074505806, 0.05000000074505806))) + vec3<f32>(0.004000000189989805, 0.004000000189989805, 0.004000000189989805)) / ((_e25 * (_e26 + vec3<f32>(0.5, 0.5, 0.5))) + vec3<f32>(0.06000000238418579, 0.06000000238418579, 0.06000000238418579))) - vec3<f32>(0.06666666269302368, 0.06666666269302368, 0.06666666269302368)) * vec3<f32>(1.3790643215179443, 1.3790643215179443, 1.3790643215179443));
    color = vec4<f32>(_e35.x, _e35.y, _e35.z, _e23.w);
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] tex_coords: vec2<f32>) -> [[location(0)]] vec4<f32> {
    tex_coords_1 = tex_coords;
    main_1();
    let _e3 = color;
    return _e3;
}
