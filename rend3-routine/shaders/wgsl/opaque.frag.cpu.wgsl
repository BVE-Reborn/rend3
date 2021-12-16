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

struct UniformBuffer {
    uniforms: UniformData;
};

struct CPUMaterialData {
    uv_transform0_: mat3x3<f32>;
    uv_transform1_: mat3x3<f32>;
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

struct DirectionalLightBuffer {
    directional_light_header: DirectionalLightBufferHeader;
    directional_lights: [[stride(112)]] array<DirectionalLight>;
};

var<private> i_coords0_1: vec2<f32>;
[[group(2), binding(0)]]
var albedo_tex: texture_2d<f32>;
var<private> i_color_1: vec4<f32>;
var<private> i_normal_1: vec3<f32>;
[[group(2), binding(1)]]
var normal_tex: texture_2d<f32>;
var<private> i_tangent_1: vec3<f32>;
[[group(2), binding(2)]]
var roughness_tex: texture_2d<f32>;
[[group(2), binding(9)]]
var ambient_occlusion_tex: texture_2d<f32>;
[[group(2), binding(3)]]
var metallic_tex: texture_2d<f32>;
[[group(2), binding(4)]]
var reflectance_tex: texture_2d<f32>;
[[group(2), binding(5)]]
var clear_coat_tex: texture_2d<f32>;
[[group(2), binding(6)]]
var clear_coat_roughness_tex: texture_2d<f32>;
[[group(2), binding(7)]]
var emissive_tex: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
[[group(0), binding(3)]]
var<uniform> unnamed: UniformBuffer;
[[group(2), binding(10)]]
var<uniform> unnamed_1: TextureData;
var<private> o_color: vec4<f32>;
var<private> i_view_position_1: vec4<f32>;
[[group(0), binding(4)]]
var<storage> unnamed_2: DirectionalLightBuffer;
[[group(0), binding(5)]]
var shadow: texture_depth_2d_array;
[[group(0), binding(2)]]
var shadow_sampler: sampler_comparison;
var<private> i_coords1_1: vec2<f32>;
var<private> i_material_1: u32;

fn main_1() {
    var phi_2380_: vec4<f32>;
    var phi_2378_: vec4<f32>;
    var phi_2382_: vec4<f32>;
    var phi_2381_: vec4<f32>;
    var phi_2383_: vec2<f32>;
    var phi_2384_: vec3<f32>;
    var phi_2385_: vec3<f32>;
    var phi_2512_: f32;
    var phi_2452_: f32;
    var phi_2404_: f32;
    var phi_1574_: bool;
    var phi_2386_: vec2<f32>;
    var phi_2455_: f32;
    var phi_2407_: f32;
    var phi_2514_: f32;
    var phi_2467_: f32;
    var phi_2419_: f32;
    var phi_2520_: f32;
    var phi_2515_: f32;
    var phi_2456_: f32;
    var phi_2408_: f32;
    var phi_2513_: f32;
    var phi_2453_: f32;
    var phi_2405_: f32;
    var phi_2511_: f32;
    var phi_2451_: f32;
    var phi_2403_: f32;
    var phi_2420_: f32;
    var phi_2475_: f32;
    var phi_2422_: f32;
    var phi_2425_: f32;
    var phi_2477_: f32;
    var phi_2447_: f32;
    var phi_2498_: f32;
    var phi_2478_: f32;
    var phi_2426_: f32;
    var phi_2476_: f32;
    var phi_2423_: f32;
    var phi_2474_: f32;
    var phi_2421_: f32;
    var phi_2499_: f32;
    var phi_2595_: vec3<f32>;
    var phi_2658_: vec3<f32>;
    var phi_2651_: f32;
    var phi_2627_: vec3<f32>;
    var phi_2599_: vec3<f32>;
    var phi_2588_: vec3<f32>;
    var phi_2500_: f32;
    var phi_2687_: vec3<f32>;
    var phi_2686_: u32;
    var phi_1222_: bool;
    var phi_1229_: bool;
    var phi_1236_: bool;
    var phi_1244_: bool;
    var phi_1251_: bool;
    var phi_2694_: f32;
    var local: vec3<f32>;
    var local_1: vec3<f32>;
    var local_2: vec3<f32>;
    var local_3: vec3<f32>;

    let _e92 = unnamed_1.material.uv_transform0_;
    let _e94 = unnamed_1.material.albedo;
    let _e96 = unnamed_1.material.emissive;
    let _e98 = unnamed_1.material.roughness;
    let _e100 = unnamed_1.material.metallic;
    let _e102 = unnamed_1.material.reflectance;
    let _e104 = unnamed_1.material.clear_coat;
    let _e106 = unnamed_1.material.clear_coat_roughness;
    let _e108 = unnamed_1.material.ambient_occlusion;
    let _e110 = unnamed_1.material.material_flags;
    let _e112 = unnamed_1.material.texture_enable;
    let _e113 = i_coords0_1;
    let _e117 = (_e92 * vec3<f32>(_e113.x, _e113.y, 1.0));
    let _e120 = vec2<f32>(_e117.x, _e117.y);
    let _e121 = dpdx(_e120);
    let _e122 = dpdy(_e120);
    if ((bitcast<bool>((_e110 & 1u)) != bitcast<bool>(0u))) {
        if ((bitcast<bool>(((_e112 >> bitcast<u32>(0)) & 1u)) != bitcast<bool>(0u))) {
            let _e133 = textureSampleGrad(albedo_tex, primary_sampler, _e120, _e121, _e122);
            phi_2380_ = _e133;
        } else {
            phi_2380_ = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
        let _e135 = phi_2380_;
        phi_2382_ = _e135;
        if ((bitcast<bool>((_e110 & 2u)) != bitcast<bool>(0u))) {
            let _e140 = i_color_1;
            phi_2378_ = _e140;
            if ((bitcast<bool>((_e110 & 4u)) != bitcast<bool>(0u))) {
                let _e145 = _e140.xyz;
                let _e152 = mix((_e145 * vec3<f32>(0.07739938050508499, 0.07739938050508499, 0.07739938050508499)), pow(((_e145 + vec3<f32>(0.054999999701976776, 0.054999999701976776, 0.054999999701976776)) * vec3<f32>(0.9478673338890076, 0.9478673338890076, 0.9478673338890076)), vec3<f32>(2.4000000953674316, 2.4000000953674316, 2.4000000953674316)), ceil((_e145 - vec3<f32>(0.040449999272823334, 0.040449999272823334, 0.040449999272823334))));
                phi_2378_ = vec4<f32>(_e152.x, _e152.y, _e152.z, _e140.w);
            }
            let _e159 = phi_2378_;
            phi_2382_ = (_e135 * _e159);
        }
        let _e162 = phi_2382_;
        phi_2381_ = _e162;
    } else {
        phi_2381_ = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let _e164 = phi_2381_;
    let _e165 = (_e164 * _e94);
    if ((bitcast<bool>((_e110 & 4096u)) != bitcast<bool>(0u))) {
        let _e170 = i_normal_1;
        phi_2658_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2651_ = 0.0;
        phi_2627_ = normalize(_e170);
        phi_2599_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2588_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2500_ = 0.0;
    } else {
        if ((bitcast<bool>(((_e112 >> bitcast<u32>(1)) & 1u)) != bitcast<bool>(0u))) {
            let _e178 = textureSampleGrad(normal_tex, primary_sampler, _e120, _e121, _e122);
            if ((bitcast<bool>((_e110 & 8u)) != bitcast<bool>(0u))) {
                if ((bitcast<bool>((_e110 & 16u)) != bitcast<bool>(0u))) {
                    phi_2383_ = _e178.wy;
                } else {
                    phi_2383_ = _e178.xy;
                }
                let _e190 = phi_2383_;
                let _e192 = ((_e190 * 2.0) - vec2<f32>(1.0, 1.0));
                phi_2384_ = vec3<f32>(_e192.x, _e192.y, sqrt(((1.0 - (_e192.x * _e192.x)) - (_e192.y * _e192.y))));
            } else {
                phi_2384_ = normalize(((_e178.xyz * 2.0) - vec3<f32>(1.0, 1.0, 1.0)));
            }
            let _e206 = phi_2384_;
            let _e207 = i_normal_1;
            let _e209 = i_tangent_1;
            phi_2385_ = (mat3x3<f32>(_e209, cross(normalize(_e207), normalize(_e209)), _e207) * _e206);
        } else {
            let _e214 = i_normal_1;
            phi_2385_ = _e214;
        }
        let _e216 = phi_2385_;
        if ((bitcast<bool>((_e110 & 32u)) != bitcast<bool>(0u))) {
            if ((bitcast<bool>(((_e112 >> bitcast<u32>(2)) & 1u)) != bitcast<bool>(0u))) {
                let _e228 = textureSampleGrad(roughness_tex, primary_sampler, _e120, _e121, _e122);
                phi_2512_ = (_e108 * _e228.x);
                phi_2452_ = (_e98 * _e228.z);
                phi_2404_ = (_e100 * _e228.y);
            } else {
                phi_2512_ = _e108;
                phi_2452_ = _e98;
                phi_2404_ = _e100;
            }
            let _e236 = phi_2512_;
            let _e238 = phi_2452_;
            let _e240 = phi_2404_;
            phi_2511_ = _e236;
            phi_2451_ = _e238;
            phi_2403_ = _e240;
        } else {
            let _e244 = (bitcast<bool>((_e110 & 64u)) != bitcast<bool>(0u));
            phi_1574_ = _e244;
            if (!(_e244)) {
                phi_1574_ = (bitcast<bool>((_e110 & 128u)) != bitcast<bool>(0u));
            }
            let _e251 = phi_1574_;
            if (_e251) {
                if ((bitcast<bool>(((_e112 >> bitcast<u32>(2)) & 1u)) != bitcast<bool>(0u))) {
                    let _e258 = textureSampleGrad(roughness_tex, primary_sampler, _e120, _e121, _e122);
                    if (_e244) {
                        phi_2386_ = _e258.yz;
                    } else {
                        phi_2386_ = _e258.xy;
                    }
                    let _e262 = phi_2386_;
                    phi_2455_ = (_e98 * _e262.y);
                    phi_2407_ = (_e100 * _e262.x);
                } else {
                    phi_2455_ = _e108;
                    phi_2407_ = _e100;
                }
                let _e268 = phi_2455_;
                let _e270 = phi_2407_;
                if ((bitcast<bool>(((_e112 >> bitcast<u32>(9)) & 1u)) != bitcast<bool>(0u))) {
                    let _e277 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e120, _e121, _e122);
                    phi_2514_ = (_e108 * _e277.x);
                } else {
                    phi_2514_ = _e108;
                }
                let _e281 = phi_2514_;
                phi_2513_ = _e281;
                phi_2453_ = _e268;
                phi_2405_ = _e270;
            } else {
                phi_2515_ = 0.0;
                phi_2456_ = 0.0;
                phi_2408_ = 0.0;
                if ((bitcast<bool>((_e110 & 256u)) != bitcast<bool>(0u))) {
                    if ((bitcast<bool>(((_e112 >> bitcast<u32>(2)) & 1u)) != bitcast<bool>(0u))) {
                        let _e292 = textureSampleGrad(roughness_tex, primary_sampler, _e120, _e121, _e122);
                        phi_2467_ = (_e98 * _e292.x);
                    } else {
                        phi_2467_ = _e98;
                    }
                    let _e296 = phi_2467_;
                    if ((bitcast<bool>(((_e112 >> bitcast<u32>(3)) & 1u)) != bitcast<bool>(0u))) {
                        let _e303 = textureSampleGrad(metallic_tex, primary_sampler, _e120, _e121, _e122);
                        phi_2419_ = (_e100 * _e303.x);
                    } else {
                        phi_2419_ = _e100;
                    }
                    let _e307 = phi_2419_;
                    if ((bitcast<bool>(((_e112 >> bitcast<u32>(9)) & 1u)) != bitcast<bool>(0u))) {
                        let _e314 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e120, _e121, _e122);
                        phi_2520_ = (_e108 * _e314.x);
                    } else {
                        phi_2520_ = _e108;
                    }
                    let _e318 = phi_2520_;
                    phi_2515_ = _e318;
                    phi_2456_ = _e296;
                    phi_2408_ = _e307;
                }
                let _e320 = phi_2515_;
                let _e322 = phi_2456_;
                let _e324 = phi_2408_;
                phi_2513_ = _e320;
                phi_2453_ = _e322;
                phi_2405_ = _e324;
            }
            let _e326 = phi_2513_;
            let _e328 = phi_2453_;
            let _e330 = phi_2405_;
            phi_2511_ = _e326;
            phi_2451_ = _e328;
            phi_2403_ = _e330;
        }
        let _e332 = phi_2511_;
        let _e334 = phi_2451_;
        let _e336 = phi_2403_;
        if ((bitcast<bool>(((_e112 >> bitcast<u32>(4)) & 1u)) != bitcast<bool>(0u))) {
            let _e343 = textureSampleGrad(reflectance_tex, primary_sampler, _e120, _e121, _e122);
            phi_2420_ = (_e102 * _e343.x);
        } else {
            phi_2420_ = _e102;
        }
        let _e347 = phi_2420_;
        let _e348 = _e165.xyz;
        let _e349 = (1.0 - _e336);
        if ((bitcast<bool>((_e110 & 512u)) != bitcast<bool>(0u))) {
            if ((bitcast<bool>(((_e112 >> bitcast<u32>(5)) & 1u)) != bitcast<bool>(0u))) {
                let _e367 = textureSampleGrad(clear_coat_tex, primary_sampler, _e120, _e121, _e122);
                phi_2475_ = (_e106 * _e367.y);
                phi_2422_ = (_e104 * _e367.x);
            } else {
                phi_2475_ = _e106;
                phi_2422_ = _e104;
            }
            let _e373 = phi_2475_;
            let _e375 = phi_2422_;
            phi_2474_ = _e373;
            phi_2421_ = _e375;
        } else {
            if ((bitcast<bool>((_e110 & 1024u)) != bitcast<bool>(0u))) {
                if ((bitcast<bool>(((_e112 >> bitcast<u32>(5)) & 1u)) != bitcast<bool>(0u))) {
                    let _e386 = textureSampleGrad(clear_coat_tex, primary_sampler, _e120, _e121, _e122);
                    phi_2425_ = (_e104 * _e386.x);
                } else {
                    phi_2425_ = _e104;
                }
                let _e390 = phi_2425_;
                if ((bitcast<bool>(((_e112 >> bitcast<u32>(6)) & 1u)) != bitcast<bool>(0u))) {
                    let _e397 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e120, _e121, _e122);
                    phi_2477_ = (_e106 * _e397.y);
                } else {
                    phi_2477_ = _e106;
                }
                let _e401 = phi_2477_;
                phi_2476_ = _e401;
                phi_2423_ = _e390;
            } else {
                phi_2478_ = 0.0;
                phi_2426_ = 0.0;
                if ((bitcast<bool>((_e110 & 2048u)) != bitcast<bool>(0u))) {
                    if ((bitcast<bool>(((_e112 >> bitcast<u32>(5)) & 1u)) != bitcast<bool>(0u))) {
                        let _e412 = textureSampleGrad(clear_coat_tex, primary_sampler, _e120, _e121, _e122);
                        phi_2447_ = (_e104 * _e412.x);
                    } else {
                        phi_2447_ = _e104;
                    }
                    let _e416 = phi_2447_;
                    if ((bitcast<bool>(((_e112 >> bitcast<u32>(6)) & 1u)) != bitcast<bool>(0u))) {
                        let _e423 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e120, _e121, _e122);
                        phi_2498_ = (_e106 * _e423.x);
                    } else {
                        phi_2498_ = _e106;
                    }
                    let _e427 = phi_2498_;
                    phi_2478_ = _e427;
                    phi_2426_ = _e416;
                }
                let _e429 = phi_2478_;
                let _e431 = phi_2426_;
                phi_2476_ = _e429;
                phi_2423_ = _e431;
            }
            let _e433 = phi_2476_;
            let _e435 = phi_2423_;
            phi_2474_ = _e433;
            phi_2421_ = _e435;
        }
        let _e437 = phi_2474_;
        let _e439 = phi_2421_;
        phi_2499_ = _e334;
        if ((_e439 != 0.0)) {
            phi_2499_ = mix(_e334, max(_e334, _e437), _e439);
        }
        let _e444 = phi_2499_;
        if ((bitcast<bool>(((_e112 >> bitcast<u32>(7)) & 1u)) != bitcast<bool>(0u))) {
            let _e452 = textureSampleGrad(emissive_tex, primary_sampler, _e120, _e121, _e122);
            phi_2595_ = (_e96 * _e452.xyz);
        } else {
            phi_2595_ = _e96;
        }
        let _e456 = phi_2595_;
        phi_2658_ = (_e348 * _e349);
        phi_2651_ = (_e444 * _e444);
        phi_2627_ = normalize(_e216);
        phi_2599_ = ((_e348 * _e336) + vec3<f32>((((0.1599999964237213 * _e347) * _e347) * _e349)));
        phi_2588_ = _e456;
        phi_2500_ = _e332;
    }
    let _e458 = phi_2658_;
    let _e460 = phi_2651_;
    let _e462 = phi_2627_;
    let _e464 = phi_2599_;
    let _e466 = phi_2588_;
    let _e468 = phi_2500_;
    let _e471 = unnamed_1.material.material_flags;
    if ((bitcast<bool>((_e471 & 4096u)) != bitcast<bool>(0u))) {
        o_color = _e165;
    } else {
        let _e476 = i_view_position_1;
        let _e479 = -(normalize(_e476.xyz));
        phi_2687_ = _e466;
        phi_2686_ = 0u;
        loop {
            let _e481 = phi_2687_;
            let _e483 = phi_2686_;
            let _e486 = unnamed_2.directional_light_header.total_lights;
            local = _e481;
            local_1 = _e481;
            local_2 = _e481;
            if ((_e483 < _e486)) {
                let _e491 = unnamed_2.directional_lights[_e483].view_proj;
                let _e494 = unnamed.uniforms.inv_view;
                let _e496 = ((_e491 * _e494) * _e476);
                let _e499 = ((_e496.xy * 0.5) + vec2<f32>(0.5, 0.5));
                let _e502 = (1.0 - _e499.y);
                let _e505 = vec4<f32>(_e499.x, _e502, f32(_e483), _e496.z);
                let _e506 = (_e499.x < 0.0);
                phi_1222_ = _e506;
                if (!(_e506)) {
                    phi_1222_ = (_e499.x > 1.0);
                }
                let _e510 = phi_1222_;
                phi_1229_ = _e510;
                if (!(_e510)) {
                    phi_1229_ = (_e502 < 0.0);
                }
                let _e514 = phi_1229_;
                phi_1236_ = _e514;
                if (!(_e514)) {
                    phi_1236_ = (_e502 > 1.0);
                }
                let _e518 = phi_1236_;
                phi_1244_ = _e518;
                if (!(_e518)) {
                    phi_1244_ = (_e496.z < -1.0);
                }
                let _e522 = phi_1244_;
                phi_1251_ = _e522;
                if (!(_e522)) {
                    phi_1251_ = (_e496.z > 1.0);
                }
                let _e526 = phi_1251_;
                if (_e526) {
                    phi_2694_ = 1.0;
                } else {
                    let _e532 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e505.x, _e505.y), i32(_e505.z), _e496.z);
                    phi_2694_ = _e532;
                }
                let _e534 = phi_2694_;
                let _e539 = unnamed_2.directional_lights[_e483].color;
                let _e541 = unnamed_2.directional_lights[_e483].direction;
                let _e544 = unnamed.uniforms.view;
                let _e554 = normalize((mat3x3<f32>(_e544[0].xyz, _e544[1].xyz, _e544[2].xyz) * -(_e541)));
                let _e556 = normalize((_e479 + _e554));
                let _e558 = abs(dot(_e462, _e479));
                let _e559 = (_e558 + 9.999999747378752e-6);
                let _e561 = clamp(dot(_e462, _e554), 0.0, 1.0);
                let _e563 = clamp(dot(_e462, _e556), 0.0, 1.0);
                let _e568 = (_e460 * _e460);
                let _e572 = ((((_e563 * _e568) - _e563) * _e563) + 1.0);
                local_3 = (_e481 + ((((_e458 * 0.31830987334251404) + (((_e464 + ((vec3<f32>(clamp(dot(_e464, vec3<f32>(16.5, 16.5, 16.5)), 0.0, 1.0)) - _e464) * pow((1.0 - clamp(dot(_e554, _e556), 0.0, 1.0)), 5.0))) * ((_e568 / ((3.1415927410125732 * _e572) * _e572)) * (0.5 / ((_e561 * sqrt((((((-9.999999747378752e-6 - _e558) * _e568) + _e559) * _e559) + _e568))) + (_e559 * sqrt(((((-(_e561) * _e568) + _e561) * _e561) + _e568))))))) * 1.0)) * _e539) * (_e561 * (_e534 * _e468))));
                continue;
            } else {
                break;
            }
            continuing {
                let _e675 = local_3;
                phi_2687_ = _e675;
                phi_2686_ = (_e483 + bitcast<u32>(1));
            }
        }
        let _e611 = local;
        let _e614 = local_1;
        let _e617 = local_2;
        let _e622 = unnamed.uniforms.ambient;
        o_color = max(vec4<f32>(_e611.x, _e614.y, _e617.z, _e165.w), (_e622 * _e165));
    }
    return;
}

[[stage(fragment)]]
fn main([[location(3)]] i_coords0_: vec2<f32>, [[location(5)]] i_color: vec4<f32>, [[location(1)]] i_normal: vec3<f32>, [[location(2)]] i_tangent: vec3<f32>, [[location(0)]] i_view_position: vec4<f32>, [[location(4)]] i_coords1_: vec2<f32>, [[location(6)]] i_material: u32) -> [[location(0)]] vec4<f32> {
    i_coords0_1 = i_coords0_;
    i_color_1 = i_color;
    i_normal_1 = i_normal;
    i_tangent_1 = i_tangent;
    i_view_position_1 = i_view_position;
    i_coords1_1 = i_coords1_;
    i_material_1 = i_material;
    main_1();
    let _e15 = o_color;
    return _e15;
}
