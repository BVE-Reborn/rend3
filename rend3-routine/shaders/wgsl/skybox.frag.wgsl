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
    origin_view_proj: mat4x4<f32>;
    inv_view: mat4x4<f32>;
    inv_view_proj: mat4x4<f32>;
    inv_origin_view_proj: mat4x4<f32>;
    frustum: Frustum;
    ambient: vec4<f32>;
};

struct UniformBuffer {
    uniforms: UniformData;
};

var<private> i_clip_position_1: vec2<f32>;
[[group(0), binding(3)]]
var<uniform> unnamed: UniformBuffer;
[[group(1), binding(0)]]
var skybox: texture_cube<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
var<private> o_color: vec4<f32>;

fn main_1() {
    let _e12 = i_clip_position_1;
    let _e18 = unnamed.uniforms.inv_origin_view_proj;
    let _e19 = (_e18 * vec4<f32>(_e12.x, _e12.y, 1.0, 1.0));
    let _e25 = textureSample(skybox, primary_sampler, normalize((_e19.xyz / vec3<f32>(_e19.w))));
    o_color = vec4<f32>(_e25.x, _e25.y, _e25.z, 1.0);
    return;
}

[[stage(fragment)]]
fn main([[location(0)]] i_clip_position: vec2<f32>) -> [[location(0)]] vec4<f32> {
    i_clip_position_1 = i_clip_position;
    main_1();
    let _e3 = o_color;
    return _e3;
}
