[[group(1), binding(0)]]
var source: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> tex_coords1: vec2<f32>;
var<private> color: vec4<f32>;

fn main1() {
    let e30: vec2<f32> = tex_coords1;
    let e31: vec4<f32> = textureSample(source, primary_sampler, e30);
    let e33: vec3<f32> = (e31.xyz * 2.0);
    let e34: vec3<f32> = (e33 * 0.15000000596046448);
    let e43: vec3<f32> = (((((e33 * (e34 + vec3<f32>(0.05000000074505806, 0.05000000074505806, 0.05000000074505806))) + vec3<f32>(0.004000000189989805, 0.004000000189989805, 0.004000000189989805)) / ((e33 * (e34 + vec3<f32>(0.5, 0.5, 0.5))) + vec3<f32>(0.06000000238418579, 0.06000000238418579, 0.06000000238418579))) - vec3<f32>(0.06666666269302368, 0.06666666269302368, 0.06666666269302368)) * vec3<f32>(1.3790643215179443, 1.3790643215179443, 1.3790643215179443));
    let e49: vec3<f32> = vec4<f32>(e43.x, e43.y, e43.z, e31.w).xyz;
    let e56: vec3<f32> = mix((e49 * 12.920000076293945), ((pow(e49, vec3<f32>(0.41666001081466675, 0.41666001081466675, 0.41666001081466675)) * 1.0549999475479126) - vec3<f32>(0.054999999701976776, 0.054999999701976776, 0.054999999701976776)), ceil((e49 - vec3<f32>(0.0031308000907301903, 0.0031308000907301903, 0.0031308000907301903))));
    color = vec4<f32>(e56.x, e56.y, e56.z, e31.w);
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] tex_coords: vec2<f32>) -> [[location(0)]] vec4<f32> {
    tex_coords1 = tex_coords;
    main1();
    let e3: vec4<f32> = color;
    return e3;
}
