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

struct CPUMaterialData {
    uv_transform0: mat3x3<f32>;
    uv_transform1: mat3x3<f32>;
    albedo: vec4<f32>;
    emissive: vec3<f32>;
    roughness: f32;
    metallic: f32;
    reflectance: f32;
    clear_coat: f32;
    clear_coat_roughness: f32;
    anisotropy: f32;
    ambient_occlusion: f32;
    alpha_cutout: f32;
    material_flags: u32;
    texture_enable: u32;
};

[[block]]
struct TextureData {
    material: CPUMaterialData;
};

struct DirectionalLightBufferHeader {
    total_lights: u32;
};

struct DirectionalLight {
    view_proj: mat4x4<f32>;
    color: vec3<f32>;
    direction: vec3<f32>;
    offset: vec2<f32>;
    size: f32;
};

[[block]]
struct DirectionalLightBuffer {
    directional_light_header: DirectionalLightBufferHeader;
    directional_lights: [[stride(112)]] array<DirectionalLight>;
};

var<private> i_coords0_1: vec2<f32>;
[[group(1), binding(0)]]
var albedo_tex: texture_2d<f32>;
var<private> i_color1: vec4<f32>;
var<private> i_normal1: vec3<f32>;
[[group(1), binding(1)]]
var normal_tex: texture_2d<f32>;
var<private> i_tangent1: vec3<f32>;
[[group(1), binding(2)]]
var roughness_tex: texture_2d<f32>;
[[group(1), binding(9)]]
var ambient_occlusion_tex: texture_2d<f32>;
[[group(1), binding(3)]]
var metallic_tex: texture_2d<f32>;
[[group(1), binding(4)]]
var reflectance_tex: texture_2d<f32>;
[[group(1), binding(5)]]
var clear_coat_tex: texture_2d<f32>;
[[group(1), binding(6)]]
var clear_coat_roughness_tex: texture_2d<f32>;
[[group(1), binding(7)]]
var emissive_tex: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
[[group(0), binding(6)]]
var<uniform> global: UniformBuffer;
[[group(1), binding(10)]]
var<uniform> global1: TextureData;
var<private> o_color: vec4<f32>;
var<private> i_view_position1: vec4<f32>;
[[group(0), binding(4)]]
var<storage> global2: DirectionalLightBuffer;
[[group(0), binding(5)]]
var shadow: texture_depth_2d_array;
[[group(0), binding(2)]]
var shadow_sampler: sampler_comparison;
var<private> i_coords1_1: vec2<f32>;
var<private> i_material1: u32;

fn main1() {
    var phi_2380: vec4<f32>;
    var phi_2378: vec4<f32>;
    var phi_2382: vec4<f32>;
    var phi_2381: vec4<f32>;
    var phi_2383: vec2<f32>;
    var phi_2384: vec3<f32>;
    var phi_2385: vec3<f32>;
    var phi_2512: f32;
    var phi_2452: f32;
    var phi_2404: f32;
    var phi_1574: bool;
    var phi_2386: vec2<f32>;
    var phi_2455: f32;
    var phi_2407: f32;
    var phi_2514: f32;
    var phi_2467: f32;
    var phi_2419: f32;
    var phi_2520: f32;
    var phi_2515: f32;
    var phi_2456: f32;
    var phi_2408: f32;
    var phi_2513: f32;
    var phi_2453: f32;
    var phi_2405: f32;
    var phi_2511: f32;
    var phi_2451: f32;
    var phi_2403: f32;
    var phi_2420: f32;
    var phi_2475: f32;
    var phi_2422: f32;
    var phi_2425: f32;
    var phi_2477: f32;
    var phi_2447: f32;
    var phi_2498: f32;
    var phi_2478: f32;
    var phi_2426: f32;
    var phi_2476: f32;
    var phi_2423: f32;
    var phi_2474: f32;
    var phi_2421: f32;
    var phi_2499: f32;
    var phi_2595: vec3<f32>;
    var phi_2658: vec3<f32>;
    var phi_2651: f32;
    var phi_2627: vec3<f32>;
    var phi_2599: vec3<f32>;
    var phi_2588: vec3<f32>;
    var phi_2500: f32;
    var phi_2687: vec3<f32>;
    var phi_2686: u32;
    var phi_1222: bool;
    var phi_1229: bool;
    var phi_1236: bool;
    var phi_1244: bool;
    var phi_1251: bool;
    var phi_2694: f32;
    var local: vec3<f32>;
    var local1: vec3<f32>;
    var local2: vec3<f32>;
    var local3: vec3<f32>;

    let e92: mat3x3<f32> = global1.material.uv_transform0;
    let e94: vec4<f32> = global1.material.albedo;
    let e96: vec3<f32> = global1.material.emissive;
    let e98: f32 = global1.material.roughness;
    let e100: f32 = global1.material.metallic;
    let e102: f32 = global1.material.reflectance;
    let e104: f32 = global1.material.clear_coat;
    let e106: f32 = global1.material.clear_coat_roughness;
    let e108: f32 = global1.material.ambient_occlusion;
    let e110: u32 = global1.material.material_flags;
    let e112: u32 = global1.material.texture_enable;
    let e113: vec2<f32> = i_coords0_1;
    let e117: vec3<f32> = (e92 * vec3<f32>(e113.x, e113.y, 1.0));
    let e120: vec2<f32> = vec2<f32>(e117.x, e117.y);
    let e121: vec2<f32> = dpdx(e120);
    let e122: vec2<f32> = dpdy(e120);
    if ((bitcast<i32>((e110 & 1u)) != bitcast<i32>(0u))) {
        if ((bitcast<i32>(((e112 >> bitcast<u32>(0)) & 1u)) != bitcast<i32>(0u))) {
            let e133: vec4<f32> = textureSampleGrad(albedo_tex, primary_sampler, e120, e121, e122);
            phi_2380 = e133;
        } else {
            phi_2380 = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
        let e135: vec4<f32> = phi_2380;
        phi_2382 = e135;
        if ((bitcast<i32>((e110 & 2u)) != bitcast<i32>(0u))) {
            let e140: vec4<f32> = i_color1;
            phi_2378 = e140;
            if ((bitcast<i32>((e110 & 4u)) != bitcast<i32>(0u))) {
                let e145: vec3<f32> = e140.xyz;
                let e152: vec3<f32> = mix((e145 * vec3<f32>(0.07739938050508499, 0.07739938050508499, 0.07739938050508499)), pow(((e145 + vec3<f32>(0.054999999701976776, 0.054999999701976776, 0.054999999701976776)) * vec3<f32>(0.9478673338890076, 0.9478673338890076, 0.9478673338890076)), vec3<f32>(2.4000000953674316, 2.4000000953674316, 2.4000000953674316)), ceil((e145 - vec3<f32>(0.040449999272823334, 0.040449999272823334, 0.040449999272823334))));
                phi_2378 = vec4<f32>(e152.x, e152.y, e152.z, e140.w);
            }
            let e159: vec4<f32> = phi_2378;
            phi_2382 = (e135 * e159);
        }
        let e162: vec4<f32> = phi_2382;
        phi_2381 = e162;
    } else {
        phi_2381 = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let e164: vec4<f32> = phi_2381;
    let e165: vec4<f32> = (e164 * e94);
    if ((bitcast<i32>((e110 & 4096u)) != bitcast<i32>(0u))) {
        let e170: vec3<f32> = i_normal1;
        phi_2658 = vec3<f32>(0.0, 0.0, 0.0);
        phi_2651 = 0.0;
        phi_2627 = normalize(e170);
        phi_2599 = vec3<f32>(0.0, 0.0, 0.0);
        phi_2588 = vec3<f32>(0.0, 0.0, 0.0);
        phi_2500 = 0.0;
    } else {
        if ((bitcast<i32>(((e112 >> bitcast<u32>(1)) & 1u)) != bitcast<i32>(0u))) {
            let e178: vec4<f32> = textureSampleGrad(normal_tex, primary_sampler, e120, e121, e122);
            if ((bitcast<i32>((e110 & 8u)) != bitcast<i32>(0u))) {
                if ((bitcast<i32>((e110 & 16u)) != bitcast<i32>(0u))) {
                    phi_2383 = e178.wy;
                } else {
                    phi_2383 = e178.xy;
                }
                let e190: vec2<f32> = phi_2383;
                let e192: vec2<f32> = ((e190 * 2.0) - vec2<f32>(1.0, 1.0));
                phi_2384 = vec3<f32>(e192.x, e192.y, sqrt(((1.0 - (e192.x * e192.x)) - (e192.y * e192.y))));
            } else {
                phi_2384 = normalize(((e178.xyz * 2.0) - vec3<f32>(1.0, 1.0, 1.0)));
            }
            let e206: vec3<f32> = phi_2384;
            let e207: vec3<f32> = i_normal1;
            let e209: vec3<f32> = i_tangent1;
            phi_2385 = (mat3x3<f32>(e209, cross(normalize(e207), normalize(e209)), e207) * e206);
        } else {
            let e214: vec3<f32> = i_normal1;
            phi_2385 = e214;
        }
        let e216: vec3<f32> = phi_2385;
        if ((bitcast<i32>((e110 & 32u)) != bitcast<i32>(0u))) {
            if ((bitcast<i32>(((e112 >> bitcast<u32>(2)) & 1u)) != bitcast<i32>(0u))) {
                let e228: vec4<f32> = textureSampleGrad(roughness_tex, primary_sampler, e120, e121, e122);
                phi_2512 = (e108 * e228.x);
                phi_2452 = (e98 * e228.z);
                phi_2404 = (e100 * e228.y);
            } else {
                phi_2512 = e108;
                phi_2452 = e98;
                phi_2404 = e100;
            }
            let e236: f32 = phi_2512;
            let e238: f32 = phi_2452;
            let e240: f32 = phi_2404;
            phi_2511 = e236;
            phi_2451 = e238;
            phi_2403 = e240;
        } else {
            let e244: bool = (bitcast<i32>((e110 & 64u)) != bitcast<i32>(0u));
            phi_1574 = e244;
            if (!(e244)) {
                phi_1574 = (bitcast<i32>((e110 & 128u)) != bitcast<i32>(0u));
            }
            let e251: bool = phi_1574;
            if (e251) {
                if ((bitcast<i32>(((e112 >> bitcast<u32>(2)) & 1u)) != bitcast<i32>(0u))) {
                    let e258: vec4<f32> = textureSampleGrad(roughness_tex, primary_sampler, e120, e121, e122);
                    if (e244) {
                        phi_2386 = e258.yz;
                    } else {
                        phi_2386 = e258.xy;
                    }
                    let e262: vec2<f32> = phi_2386;
                    phi_2455 = (e98 * e262.y);
                    phi_2407 = (e100 * e262.x);
                } else {
                    phi_2455 = e108;
                    phi_2407 = e100;
                }
                let e268: f32 = phi_2455;
                let e270: f32 = phi_2407;
                if ((bitcast<i32>(((e112 >> bitcast<u32>(9)) & 1u)) != bitcast<i32>(0u))) {
                    let e277: vec4<f32> = textureSampleGrad(ambient_occlusion_tex, primary_sampler, e120, e121, e122);
                    phi_2514 = (e108 * e277.x);
                } else {
                    phi_2514 = e108;
                }
                let e281: f32 = phi_2514;
                phi_2513 = e281;
                phi_2453 = e268;
                phi_2405 = e270;
            } else {
                phi_2515 = 0.0;
                phi_2456 = 0.0;
                phi_2408 = 0.0;
                if ((bitcast<i32>((e110 & 256u)) != bitcast<i32>(0u))) {
                    if ((bitcast<i32>(((e112 >> bitcast<u32>(2)) & 1u)) != bitcast<i32>(0u))) {
                        let e292: vec4<f32> = textureSampleGrad(roughness_tex, primary_sampler, e120, e121, e122);
                        phi_2467 = (e98 * e292.x);
                    } else {
                        phi_2467 = e98;
                    }
                    let e296: f32 = phi_2467;
                    if ((bitcast<i32>(((e112 >> bitcast<u32>(3)) & 1u)) != bitcast<i32>(0u))) {
                        let e303: vec4<f32> = textureSampleGrad(metallic_tex, primary_sampler, e120, e121, e122);
                        phi_2419 = (e100 * e303.x);
                    } else {
                        phi_2419 = e100;
                    }
                    let e307: f32 = phi_2419;
                    if ((bitcast<i32>(((e112 >> bitcast<u32>(9)) & 1u)) != bitcast<i32>(0u))) {
                        let e314: vec4<f32> = textureSampleGrad(ambient_occlusion_tex, primary_sampler, e120, e121, e122);
                        phi_2520 = (e108 * e314.x);
                    } else {
                        phi_2520 = e108;
                    }
                    let e318: f32 = phi_2520;
                    phi_2515 = e318;
                    phi_2456 = e296;
                    phi_2408 = e307;
                }
                let e320: f32 = phi_2515;
                let e322: f32 = phi_2456;
                let e324: f32 = phi_2408;
                phi_2513 = e320;
                phi_2453 = e322;
                phi_2405 = e324;
            }
            let e326: f32 = phi_2513;
            let e328: f32 = phi_2453;
            let e330: f32 = phi_2405;
            phi_2511 = e326;
            phi_2451 = e328;
            phi_2403 = e330;
        }
        let e332: f32 = phi_2511;
        let e334: f32 = phi_2451;
        let e336: f32 = phi_2403;
        if ((bitcast<i32>(((e112 >> bitcast<u32>(4)) & 1u)) != bitcast<i32>(0u))) {
            let e343: vec4<f32> = textureSampleGrad(reflectance_tex, primary_sampler, e120, e121, e122);
            phi_2420 = (e102 * e343.x);
        } else {
            phi_2420 = e102;
        }
        let e347: f32 = phi_2420;
        let e348: vec3<f32> = e165.xyz;
        let e349: f32 = (1.0 - e336);
        if ((bitcast<i32>((e110 & 512u)) != bitcast<i32>(0u))) {
            if ((bitcast<i32>(((e112 >> bitcast<u32>(5)) & 1u)) != bitcast<i32>(0u))) {
                let e367: vec4<f32> = textureSampleGrad(clear_coat_tex, primary_sampler, e120, e121, e122);
                phi_2475 = (e106 * e367.y);
                phi_2422 = (e104 * e367.x);
            } else {
                phi_2475 = e106;
                phi_2422 = e104;
            }
            let e373: f32 = phi_2475;
            let e375: f32 = phi_2422;
            phi_2474 = e373;
            phi_2421 = e375;
        } else {
            if ((bitcast<i32>((e110 & 1024u)) != bitcast<i32>(0u))) {
                if ((bitcast<i32>(((e112 >> bitcast<u32>(5)) & 1u)) != bitcast<i32>(0u))) {
                    let e386: vec4<f32> = textureSampleGrad(clear_coat_tex, primary_sampler, e120, e121, e122);
                    phi_2425 = (e104 * e386.x);
                } else {
                    phi_2425 = e104;
                }
                let e390: f32 = phi_2425;
                if ((bitcast<i32>(((e112 >> bitcast<u32>(6)) & 1u)) != bitcast<i32>(0u))) {
                    let e397: vec4<f32> = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, e120, e121, e122);
                    phi_2477 = (e106 * e397.y);
                } else {
                    phi_2477 = e106;
                }
                let e401: f32 = phi_2477;
                phi_2476 = e401;
                phi_2423 = e390;
            } else {
                phi_2478 = 0.0;
                phi_2426 = 0.0;
                if ((bitcast<i32>((e110 & 2048u)) != bitcast<i32>(0u))) {
                    if ((bitcast<i32>(((e112 >> bitcast<u32>(5)) & 1u)) != bitcast<i32>(0u))) {
                        let e412: vec4<f32> = textureSampleGrad(clear_coat_tex, primary_sampler, e120, e121, e122);
                        phi_2447 = (e104 * e412.x);
                    } else {
                        phi_2447 = e104;
                    }
                    let e416: f32 = phi_2447;
                    if ((bitcast<i32>(((e112 >> bitcast<u32>(6)) & 1u)) != bitcast<i32>(0u))) {
                        let e423: vec4<f32> = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, e120, e121, e122);
                        phi_2498 = (e106 * e423.x);
                    } else {
                        phi_2498 = e106;
                    }
                    let e427: f32 = phi_2498;
                    phi_2478 = e427;
                    phi_2426 = e416;
                }
                let e429: f32 = phi_2478;
                let e431: f32 = phi_2426;
                phi_2476 = e429;
                phi_2423 = e431;
            }
            let e433: f32 = phi_2476;
            let e435: f32 = phi_2423;
            phi_2474 = e433;
            phi_2421 = e435;
        }
        let e437: f32 = phi_2474;
        let e439: f32 = phi_2421;
        phi_2499 = e334;
        if ((e439 != 0.0)) {
            phi_2499 = mix(e334, max(e334, e437), e439);
        }
        let e444: f32 = phi_2499;
        if ((bitcast<i32>(((e112 >> bitcast<u32>(7)) & 1u)) != bitcast<i32>(0u))) {
            let e452: vec4<f32> = textureSampleGrad(emissive_tex, primary_sampler, e120, e121, e122);
            phi_2595 = (e96 * e452.xyz);
        } else {
            phi_2595 = e96;
        }
        let e456: vec3<f32> = phi_2595;
        phi_2658 = (e348 * e349);
        phi_2651 = (e444 * e444);
        phi_2627 = normalize(e216);
        phi_2599 = ((e348 * e336) + vec3<f32>((((0.1599999964237213 * e347) * e347) * e349)));
        phi_2588 = e456;
        phi_2500 = e332;
    }
    let e458: vec3<f32> = phi_2658;
    let e460: f32 = phi_2651;
    let e462: vec3<f32> = phi_2627;
    let e464: vec3<f32> = phi_2599;
    let e466: vec3<f32> = phi_2588;
    let e468: f32 = phi_2500;
    let e471: u32 = global1.material.material_flags;
    if ((bitcast<i32>((e471 & 4096u)) != bitcast<i32>(0u))) {
        o_color = e165;
    } else {
        let e476: vec4<f32> = i_view_position1;
        let e479: vec3<f32> = -(normalize(e476.xyz));
        phi_2687 = e466;
        phi_2686 = 0u;
        loop {
            let e481: vec3<f32> = phi_2687;
            let e483: u32 = phi_2686;
            let e486: u32 = global2.directional_light_header.total_lights;
            local = e481;
            local1 = e481;
            local2 = e481;
            if ((e483 < e486)) {
                let e491: mat4x4<f32> = global2.directional_lights[e483].view_proj;
                let e494: mat4x4<f32> = global.uniforms.inv_view;
                let e496: vec4<f32> = ((e491 * e494) * e476);
                let e499: vec2<f32> = ((e496.xy * 0.5) + vec2<f32>(0.5, 0.5));
                let e502: f32 = (1.0 - e499.y);
                let e505: vec4<f32> = vec4<f32>(e499.x, e502, f32(e483), e496.z);
                let e506: bool = (e499.x < 0.0);
                phi_1222 = e506;
                if (!(e506)) {
                    phi_1222 = (e499.x > 1.0);
                }
                let e510: bool = phi_1222;
                phi_1229 = e510;
                if (!(e510)) {
                    phi_1229 = (e502 < 0.0);
                }
                let e514: bool = phi_1229;
                phi_1236 = e514;
                if (!(e514)) {
                    phi_1236 = (e502 > 1.0);
                }
                let e518: bool = phi_1236;
                phi_1244 = e518;
                if (!(e518)) {
                    phi_1244 = (e496.z < -1.0);
                }
                let e522: bool = phi_1244;
                phi_1251 = e522;
                if (!(e522)) {
                    phi_1251 = (e496.z > 1.0);
                }
                let e526: bool = phi_1251;
                if (e526) {
                    phi_2694 = 1.0;
                } else {
                    let e532: f32 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(e505.x, e505.y), i32(e505.z), e496.z);
                    phi_2694 = e532;
                }
                let e534: f32 = phi_2694;
                let e539: vec3<f32> = global2.directional_lights[e483].color;
                let e541: vec3<f32> = global2.directional_lights[e483].direction;
                let e544: mat4x4<f32> = global.uniforms.view;
                let e554: vec3<f32> = normalize((mat3x3<f32>(e544[0].xyz, e544[1].xyz, e544[2].xyz) * -(e541)));
                let e556: vec3<f32> = normalize((e479 + e554));
                let e558: f32 = abs(dot(e462, e479));
                let e559: f32 = (e558 + 0.000009999999747378752);
                let e561: f32 = clamp(dot(e462, e554), 0.0, 1.0);
                let e563: f32 = clamp(dot(e462, e556), 0.0, 1.0);
                let e568: f32 = (e460 * e460);
                let e572: f32 = ((((e563 * e568) - e563) * e563) + 1.0);
                local3 = (e481 + ((((e458 * 0.31830987334251404) + (((e464 + ((vec3<f32>(clamp(dot(e464, vec3<f32>(16.5, 16.5, 16.5)), 0.0, 1.0)) - e464) * pow((1.0 - clamp(dot(e554, e556), 0.0, 1.0)), 5.0))) * ((e568 / ((3.1415927410125732 * e572) * e572)) * (0.5 / ((e561 * sqrt((((((-0.000009999999747378752 - e558) * e568) + e559) * e559) + e568))) + (e559 * sqrt(((((-(e561) * e568) + e561) * e561) + e568))))))) * 1.0)) * e539) * (e561 * (e534 * e468))));
                continue;
            } else {
                break;
            }
            continuing {
                let e675: vec3<f32> = local3;
                phi_2687 = e675;
                phi_2686 = (e483 + bitcast<u32>(1));
            }
        }
        let e611: vec3<f32> = local;
        let e614: vec3<f32> = local1;
        let e617: vec3<f32> = local2;
        let e622: vec4<f32> = global.uniforms.ambient;
        o_color = max(vec4<f32>(e611.x, e614.y, e617.z, e165.w), (e622 * e165));
    }
    return;
}

[[stage(fragment)]]
fn main([[location(3)]] i_coords0: vec2<f32>, [[location(5)]] i_color: vec4<f32>, [[location(1)]] i_normal: vec3<f32>, [[location(2)]] i_tangent: vec3<f32>, [[location(0)]] i_view_position: vec4<f32>, [[location(4)]] i_coords1: vec2<f32>, [[location(6)]] i_material: u32) -> [[location(0)]] vec4<f32> {
    i_coords0_1 = i_coords0;
    i_color1 = i_color;
    i_normal1 = i_normal;
    i_tangent1 = i_tangent;
    i_view_position1 = i_view_position;
    i_coords1_1 = i_coords1;
    i_material1 = i_material;
    main1();
    let e15: vec4<f32> = o_color;
    return e15;
}
