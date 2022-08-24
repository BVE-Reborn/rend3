fn toLinear( sRGB: vec4<f32>) -> vec4<f32>
{
  let cutoff: vec3<bool> = sRGB.rgb < vec3<f32>(0.04045);
  let higher: vec3<f32> = pow((sRGB.rgb + vec3<f32>(0.055))/vec3<f32>(1.055), vec3<f32>(2.4));
  let lower: vec3<f32> = sRGB.rgb/vec3<f32>(12.92);

  return vec4<f32>(select(higher, lower, cutoff), sRGB.a);
}

struct VertexOutput {
@location(0) color: vec4<f32>,
@location(1) tc: vec2<f32> ,
@builtin(position) pos: vec4<f32>,
};

struct VertexInput {
@location(0) pos: vec2<f32>,
@location(1) tc: vec2<f32>,
@location(2) color: vec4<f32>,
};

struct Uni {
u_screen_size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> uni: Uni;


@vertex
fn vs_main(input: VertexInput ) -> VertexOutput {
let pos = input.pos;
var output: VertexOutput;
  output.pos = vec4<f32>(2.0 * pos.x / uni.u_screen_size.x - 1.0,
                      1.0 - 2.0 * pos.y / uni.u_screen_size.y , 0.0 , 1.0) ;
  output.color = toLinear(input.color);
  output.tc =  input.tc;
  return output;
}


@group(1) @binding(0) var u_sampler: sampler;
@group(1) @binding(1) var u_texture: texture_2d<f32>;

@fragment
fn fs_main(
in: VertexOutput
)
-> @location(0) vec4<f32>
{
return in.color *
textureSample(u_texture, u_sampler, in.tc);
}
