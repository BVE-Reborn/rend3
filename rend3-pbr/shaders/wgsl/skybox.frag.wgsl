struct Plane {
    inner: vec4<f32>;
};

struct Frustum {
    left: Plane;
    right: Plane;
    top: Plane;
    bottom: Plane;
    near: Plane;
};

struct UniformData {
    view: mat4x4<f32>;
    view_proj: mat4x4<f32>;
    inv_view: mat4x4<f32>;
    inv_origin_view_proj: mat4x4<f32>;
    frustum: Frustum;
    ambient: vec4<f32>;
};

[[block]]
struct UniformBuffer {
    uniforms: UniformData;
};

var<private> i_clip_position1: vec2<f32>;
[[group(0), binding(3)]]
var<uniform> global: UniformBuffer;
[[group(1), binding(0)]]
var skybox: texture_cube<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> o_color: vec4<f32>;

fn main1() {
    let e12: vec2<f32> = i_clip_position1;
    let e18: mat4x4<f32> = global.uniforms.inv_origin_view_proj;
    let e19: vec4<f32> = (e18 * vec4<f32>(e12.x, e12.y, 1.0, 1.0));
    let e25: vec4<f32> = textureSample(skybox, primary_sampler, normalize((e19.xyz / vec3<f32>(e19.w))));
    o_color = vec4<f32>(e25.x, e25.y, e25.z, 1.0);
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] i_clip_position: vec2<f32>) -> [[location(0)]] vec4<f32> {
    i_clip_position1 = i_clip_position;
    main1();
    let e3: vec4<f32> = o_color;
    return e3;
}
