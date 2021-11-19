[[group(1), binding(0)]]
var source: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> tex_coords1: vec2<f32>;
var<private> color: vec4<f32>;

fn main1() {
    let e22: vec2<f32> = tex_coords1;
    let e23: vec4<f32> = textureSample(source, primary_sampler, e22);
    let e25: vec3<f32> = (e23.xyz * 2.0);
    let e26: vec3<f32> = (e25 * 0.15000000596046448);
    let e35: vec3<f32> = (((((e25 * (e26 + vec3<f32>(0.05000000074505806, 0.05000000074505806, 0.05000000074505806))) + vec3<f32>(0.004000000189989805, 0.004000000189989805, 0.004000000189989805)) / ((e25 * (e26 + vec3<f32>(0.5, 0.5, 0.5))) + vec3<f32>(0.06000000238418579, 0.06000000238418579, 0.06000000238418579))) - vec3<f32>(0.06666666269302368, 0.06666666269302368, 0.06666666269302368)) * vec3<f32>(1.3790643215179443, 1.3790643215179443, 1.3790643215179443));
    color = vec4<f32>(e35.x, e35.y, e35.z, e23.w);
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] tex_coords: vec2<f32>) -> [[location(0)]] vec4<f32> {
    tex_coords1 = tex_coords;
    main1();
    let e3: vec4<f32> = color;
    return e3;
}
