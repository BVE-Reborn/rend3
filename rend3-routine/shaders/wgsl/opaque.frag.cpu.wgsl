struct Plane {
    inner: vec4<f32>,
}

struct Frustum {
    left: Plane,
    right: Plane,
    top: Plane,
    bottom: Plane,
    near: Plane,
}

struct UniformData {
    view: mat4x4<f32>,
    view_proj: mat4x4<f32>,
    origin_view_proj: mat4x4<f32>,
    inv_view: mat4x4<f32>,
    inv_view_proj: mat4x4<f32>,
    inv_origin_view_proj: mat4x4<f32>,
    frustum: Frustum,
    ambient: vec4<f32>,
    resolution: vec2<u32>,
}

struct UniformBuffer {
    uniforms: UniformData,
}

struct CPUMaterialData {
    uv_transform0_: mat3x3<f32>,
    uv_transform1_: mat3x3<f32>,
    albedo: vec4<f32>,
    emissive: vec3<f32>,
    roughness: f32,
    metallic: f32,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    anisotropy: f32,
    ambient_occlusion: f32,
    alpha_cutout: f32,
    material_flags: u32,
    texture_enable: u32,
}

struct TextureData {
    material: CPUMaterialData,
}

struct DirectionalLightBufferHeader {
    total_lights: u32,
}

struct DirectionalLight {
    view_proj: mat4x4<f32>,
    color: vec3<f32>,
    direction: vec3<f32>,
    offset: vec2<f32>,
    size: f32,
}

struct DirectionalLightBuffer {
    directional_light_header: DirectionalLightBufferHeader,
    directional_lights: array<DirectionalLight>,
}

var<private> i_coords0_1: vec2<f32>;
@group(2) @binding(1) 
var albedo_tex: texture_2d<f32>;
var<private> i_color_1: vec4<f32>;
var<private> i_normal_1: vec3<f32>;
@group(2) @binding(2) 
var normal_tex: texture_2d<f32>;
var<private> i_tangent_1: vec3<f32>;
@group(2) @binding(3) 
var roughness_tex: texture_2d<f32>;
@group(2) @binding(10) 
var ambient_occlusion_tex: texture_2d<f32>;
@group(2) @binding(4) 
var metallic_tex: texture_2d<f32>;
@group(2) @binding(5) 
var reflectance_tex: texture_2d<f32>;
@group(2) @binding(6) 
var clear_coat_tex: texture_2d<f32>;
@group(2) @binding(7) 
var clear_coat_roughness_tex: texture_2d<f32>;
@group(2) @binding(8) 
var emissive_tex: texture_2d<f32>;
@group(0) @binding(0) 
var primary_sampler: sampler;
@group(0) @binding(3) 
var<uniform> unnamed: UniformBuffer;
@group(2) @binding(0) 
var<storage> unnamed_1: TextureData;
var<private> o_color: vec4<f32>;
var<private> i_view_position_1: vec4<f32>;
@group(0) @binding(4) 
var<storage> unnamed_2: DirectionalLightBuffer;
@group(0) @binding(5) 
var shadow: texture_depth_2d_array;
@group(0) @binding(2) 
var shadow_sampler: sampler_comparison;

fn main_1() {
    var phi_2521_: vec4<f32>;
    var phi_2519_: vec4<f32>;
    var phi_2523_: vec4<f32>;
    var phi_2522_: vec4<f32>;
    var phi_2524_: vec2<f32>;
    var phi_2525_: vec3<f32>;
    var phi_2526_: vec3<f32>;
    var phi_2527_: vec3<f32>;
    var phi_2659_: f32;
    var phi_2597_: f32;
    var phi_2547_: f32;
    var phi_1660_: bool;
    var phi_2528_: vec2<f32>;
    var phi_2600_: f32;
    var phi_2550_: f32;
    var phi_2661_: f32;
    var phi_2613_: f32;
    var phi_2563_: f32;
    var phi_2668_: f32;
    var phi_2662_: f32;
    var phi_2601_: f32;
    var phi_2551_: f32;
    var phi_2660_: f32;
    var phi_2598_: f32;
    var phi_2548_: f32;
    var phi_2658_: f32;
    var phi_2596_: f32;
    var phi_2546_: f32;
    var phi_2564_: f32;
    var phi_2621_: f32;
    var phi_2566_: f32;
    var phi_2569_: f32;
    var phi_2623_: f32;
    var phi_2592_: f32;
    var phi_2645_: f32;
    var phi_2624_: f32;
    var phi_2570_: f32;
    var phi_2622_: f32;
    var phi_2567_: f32;
    var phi_2620_: f32;
    var phi_2565_: f32;
    var phi_2646_: f32;
    var phi_2744_: vec3<f32>;
    var phi_2807_: vec3<f32>;
    var phi_2800_: f32;
    var phi_2776_: vec3<f32>;
    var phi_2748_: vec3<f32>;
    var phi_2737_: vec3<f32>;
    var phi_2647_: f32;
    var phi_2836_: vec3<f32>;
    var phi_2835_: u32;
    var phi_1305_: bool;
    var phi_1312_: bool;
    var phi_1319_: bool;
    var phi_1327_: bool;
    var phi_1334_: bool;
    var phi_2843_: f32;
    var local: vec3<f32>;
    var local_1: vec3<f32>;
    var local_2: vec3<f32>;
    var local_3: vec3<f32>;

    let _e98 = unnamed_1.material.uv_transform0_;
    let _e100 = unnamed_1.material.albedo;
    let _e102 = unnamed_1.material.emissive;
    let _e104 = unnamed_1.material.roughness;
    let _e106 = unnamed_1.material.metallic;
    let _e108 = unnamed_1.material.reflectance;
    let _e110 = unnamed_1.material.clear_coat;
    let _e112 = unnamed_1.material.clear_coat_roughness;
    let _e114 = unnamed_1.material.ambient_occlusion;
    let _e116 = unnamed_1.material.material_flags;
    let _e118 = unnamed_1.material.texture_enable;
    let _e119 = i_coords0_1;
    let _e123 = (_e98 * vec3<f32>(_e119.x, _e119.y, 1.0));
    let _e126 = vec2<f32>(_e123.x, _e123.y);
    let _e127 = dpdx(_e126);
    let _e128 = dpdy(_e126);
    if ((_e116 & 1u) != 0u) {
        if (((_e118 >> bitcast<u32>(0)) & 1u) != 0u) {
            let _e135 = textureSampleGrad(albedo_tex, primary_sampler, _e126, _e127, _e128);
            phi_2521_ = _e135;
        } else {
            phi_2521_ = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
        let _e137 = phi_2521_;
        phi_2523_ = _e137;
        if ((_e116 & 2u) != 0u) {
            let _e140 = i_color_1;
            phi_2519_ = _e140;
            if ((_e116 & 4u) != 0u) {
                let _e143 = _e140.xyz;
                let _e151 = mix((_e143 * vec3<f32>(0.07739938050508499, 0.07739938050508499, 0.07739938050508499)), pow(((_e143 + vec3<f32>(0.054999999701976776, 0.054999999701976776, 0.054999999701976776)) * vec3<f32>(0.9478673338890076, 0.9478673338890076, 0.9478673338890076)), vec3<f32>(2.4000000953674316, 2.4000000953674316, 2.4000000953674316)), clamp(ceil((_e143 - vec3<f32>(0.040449999272823334, 0.040449999272823334, 0.040449999272823334))), vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0)));
                phi_2519_ = vec4<f32>(_e151.x, _e151.y, _e151.z, _e140.w);
            }
            let _e158 = phi_2519_;
            phi_2523_ = (_e137 * _e158);
        }
        let _e161 = phi_2523_;
        phi_2522_ = _e161;
    } else {
        phi_2522_ = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let _e163 = phi_2522_;
    let _e164 = (_e163 * _e100);
    if ((_e116 & 8192u) != 0u) {
        let _e167 = i_normal_1;
        phi_2807_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2800_ = 0.0;
        phi_2776_ = normalize(_e167);
        phi_2748_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2737_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2647_ = 0.0;
    } else {
        if (((_e118 >> bitcast<u32>(1)) & 1u) != 0u) {
            let _e173 = textureSampleGrad(normal_tex, primary_sampler, _e126, _e127, _e128);
            if ((_e116 & 8u) != 0u) {
                if ((_e116 & 16u) != 0u) {
                    phi_2524_ = _e173.wy;
                } else {
                    phi_2524_ = _e173.xy;
                }
                let _e181 = phi_2524_;
                let _e183 = ((_e181 * 2.0) - vec2<f32>(1.0, 1.0));
                phi_2525_ = vec3<f32>(_e183.x, _e183.y, sqrt(((1.0 - (_e183.x * _e183.x)) - (_e183.y * _e183.y))));
            } else {
                phi_2525_ = normalize(((_e173.xyz * 2.0) - vec3<f32>(1.0, 1.0, 1.0)));
            }
            let _e197 = phi_2525_;
            phi_2526_ = _e197;
            if ((_e116 & 32u) != 0u) {
                phi_2526_ = vec3<f32>(_e197.x, -(_e197.y), _e197.z);
            }
            let _e207 = phi_2526_;
            let _e208 = i_normal_1;
            let _e209 = normalize(_e208);
            let _e210 = i_tangent_1;
            let _e211 = normalize(_e210);
            phi_2527_ = (mat3x3<f32>(_e211, cross(_e209, _e211), _e209) * _e207);
        } else {
            let _e215 = i_normal_1;
            phi_2527_ = _e215;
        }
        let _e217 = phi_2527_;
        if ((_e116 & 64u) != 0u) {
            if (((_e118 >> bitcast<u32>(2)) & 1u) != 0u) {
                let _e225 = textureSampleGrad(roughness_tex, primary_sampler, _e126, _e127, _e128);
                phi_2659_ = (_e114 * _e225.x);
                phi_2597_ = (_e104 * _e225.y);
                phi_2547_ = (_e106 * _e225.z);
            } else {
                phi_2659_ = _e114;
                phi_2597_ = _e104;
                phi_2547_ = _e106;
            }
            let _e233 = phi_2659_;
            let _e235 = phi_2597_;
            let _e237 = phi_2547_;
            phi_2658_ = _e233;
            phi_2596_ = _e235;
            phi_2546_ = _e237;
        } else {
            let _e239 = ((_e116 & 128u) != 0u);
            phi_1660_ = _e239;
            if !(_e239) {
                phi_1660_ = ((_e116 & 256u) != 0u);
            }
            let _e244 = phi_1660_;
            if _e244 {
                if (((_e118 >> bitcast<u32>(2)) & 1u) != 0u) {
                    let _e249 = textureSampleGrad(roughness_tex, primary_sampler, _e126, _e127, _e128);
                    if _e239 {
                        phi_2528_ = _e249.yz;
                    } else {
                        phi_2528_ = _e249.xy;
                    }
                    let _e253 = phi_2528_;
                    phi_2600_ = (_e104 * _e253.x);
                    phi_2550_ = (_e106 * _e253.y);
                } else {
                    phi_2600_ = _e104;
                    phi_2550_ = _e106;
                }
                let _e259 = phi_2600_;
                let _e261 = phi_2550_;
                if (((_e118 >> bitcast<u32>(9)) & 1u) != 0u) {
                    let _e266 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e126, _e127, _e128);
                    phi_2661_ = (_e114 * _e266.x);
                } else {
                    phi_2661_ = _e114;
                }
                let _e270 = phi_2661_;
                phi_2660_ = _e270;
                phi_2598_ = _e259;
                phi_2548_ = _e261;
            } else {
                phi_2662_ = 0.0;
                phi_2601_ = 0.0;
                phi_2551_ = 0.0;
                if ((_e116 & 512u) != 0u) {
                    if (((_e118 >> bitcast<u32>(2)) & 1u) != 0u) {
                        let _e277 = textureSampleGrad(roughness_tex, primary_sampler, _e126, _e127, _e128);
                        phi_2613_ = (_e104 * _e277.x);
                    } else {
                        phi_2613_ = _e104;
                    }
                    let _e281 = phi_2613_;
                    if (((_e118 >> bitcast<u32>(3)) & 1u) != 0u) {
                        let _e286 = textureSampleGrad(metallic_tex, primary_sampler, _e126, _e127, _e128);
                        phi_2563_ = (_e106 * _e286.x);
                    } else {
                        phi_2563_ = _e106;
                    }
                    let _e290 = phi_2563_;
                    if (((_e118 >> bitcast<u32>(9)) & 1u) != 0u) {
                        let _e295 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e126, _e127, _e128);
                        phi_2668_ = (_e114 * _e295.x);
                    } else {
                        phi_2668_ = _e114;
                    }
                    let _e299 = phi_2668_;
                    phi_2662_ = _e299;
                    phi_2601_ = _e281;
                    phi_2551_ = _e290;
                }
                let _e301 = phi_2662_;
                let _e303 = phi_2601_;
                let _e305 = phi_2551_;
                phi_2660_ = _e301;
                phi_2598_ = _e303;
                phi_2548_ = _e305;
            }
            let _e307 = phi_2660_;
            let _e309 = phi_2598_;
            let _e311 = phi_2548_;
            phi_2658_ = _e307;
            phi_2596_ = _e309;
            phi_2546_ = _e311;
        }
        let _e313 = phi_2658_;
        let _e315 = phi_2596_;
        let _e317 = phi_2546_;
        if (((_e118 >> bitcast<u32>(4)) & 1u) != 0u) {
            let _e322 = textureSampleGrad(reflectance_tex, primary_sampler, _e126, _e127, _e128);
            phi_2564_ = (_e108 * _e322.x);
        } else {
            phi_2564_ = _e108;
        }
        let _e326 = phi_2564_;
        let _e327 = _e164.xyz;
        let _e328 = (1.0 - _e317);
        if ((_e116 & 1024u) != 0u) {
            if (((_e118 >> bitcast<u32>(5)) & 1u) != 0u) {
                let _e342 = textureSampleGrad(clear_coat_tex, primary_sampler, _e126, _e127, _e128);
                phi_2621_ = (_e112 * _e342.y);
                phi_2566_ = (_e110 * _e342.x);
            } else {
                phi_2621_ = _e112;
                phi_2566_ = _e110;
            }
            let _e348 = phi_2621_;
            let _e350 = phi_2566_;
            phi_2620_ = _e348;
            phi_2565_ = _e350;
        } else {
            if ((_e116 & 2048u) != 0u) {
                if (((_e118 >> bitcast<u32>(5)) & 1u) != 0u) {
                    let _e357 = textureSampleGrad(clear_coat_tex, primary_sampler, _e126, _e127, _e128);
                    phi_2569_ = (_e110 * _e357.x);
                } else {
                    phi_2569_ = _e110;
                }
                let _e361 = phi_2569_;
                if (((_e118 >> bitcast<u32>(6)) & 1u) != 0u) {
                    let _e366 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e126, _e127, _e128);
                    phi_2623_ = (_e112 * _e366.y);
                } else {
                    phi_2623_ = _e112;
                }
                let _e370 = phi_2623_;
                phi_2622_ = _e370;
                phi_2567_ = _e361;
            } else {
                phi_2624_ = 0.0;
                phi_2570_ = 0.0;
                if ((_e116 & 4096u) != 0u) {
                    if (((_e118 >> bitcast<u32>(5)) & 1u) != 0u) {
                        let _e377 = textureSampleGrad(clear_coat_tex, primary_sampler, _e126, _e127, _e128);
                        phi_2592_ = (_e110 * _e377.x);
                    } else {
                        phi_2592_ = _e110;
                    }
                    let _e381 = phi_2592_;
                    if (((_e118 >> bitcast<u32>(6)) & 1u) != 0u) {
                        let _e386 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e126, _e127, _e128);
                        phi_2645_ = (_e112 * _e386.x);
                    } else {
                        phi_2645_ = _e112;
                    }
                    let _e390 = phi_2645_;
                    phi_2624_ = _e390;
                    phi_2570_ = _e381;
                }
                let _e392 = phi_2624_;
                let _e394 = phi_2570_;
                phi_2622_ = _e392;
                phi_2567_ = _e394;
            }
            let _e396 = phi_2622_;
            let _e398 = phi_2567_;
            phi_2620_ = _e396;
            phi_2565_ = _e398;
        }
        let _e400 = phi_2620_;
        let _e402 = phi_2565_;
        phi_2646_ = _e315;
        if (_e402 != 0.0) {
            phi_2646_ = mix(_e315, max(_e315, _e400), _e402);
        }
        let _e407 = phi_2646_;
        if (((_e118 >> bitcast<u32>(7)) & 1u) != 0u) {
            let _e413 = textureSampleGrad(emissive_tex, primary_sampler, _e126, _e127, _e128);
            phi_2744_ = (_e102 * _e413.xyz);
        } else {
            phi_2744_ = _e102;
        }
        let _e417 = phi_2744_;
        phi_2807_ = (_e327 * _e328);
        phi_2800_ = (_e407 * _e407);
        phi_2776_ = normalize(_e217);
        phi_2748_ = ((_e327 * _e317) + vec3<f32>((((0.1599999964237213 * _e326) * _e326) * _e328)));
        phi_2737_ = _e417;
        phi_2647_ = _e313;
    }
    let _e419 = phi_2807_;
    let _e421 = phi_2800_;
    let _e423 = phi_2776_;
    let _e425 = phi_2748_;
    let _e427 = phi_2737_;
    let _e429 = phi_2647_;
    let _e432 = unnamed_1.material.material_flags;
    if ((_e432 & 8192u) != 0u) {
        o_color = _e164;
    } else {
        let _e435 = i_view_position_1;
        let _e438 = -(normalize(_e435.xyz));
        phi_2836_ = _e427;
        phi_2835_ = 0u;
        loop {
            let _e440 = phi_2836_;
            let _e442 = phi_2835_;
            let _e445 = unnamed_2.directional_light_header.total_lights;
            local = _e440;
            local_1 = _e440;
            local_2 = _e440;
            if (_e442 < _e445) {
                let _e450 = unnamed_2.directional_lights[_e442].view_proj;
                let _e453 = unnamed.uniforms.inv_view;
                let _e455 = ((_e450 * _e453) * _e435);
                let _e458 = ((_e455.xy * 0.5) + vec2<f32>(0.5, 0.5));
                let _e461 = (1.0 - _e458.y);
                let _e464 = vec4<f32>(_e458.x, _e461, f32(_e442), _e455.z);
                let _e465 = (_e458.x < 0.0);
                phi_1305_ = _e465;
                if !(_e465) {
                    phi_1305_ = (_e458.x > 1.0);
                }
                let _e469 = phi_1305_;
                phi_1312_ = _e469;
                if !(_e469) {
                    phi_1312_ = (_e461 < 0.0);
                }
                let _e473 = phi_1312_;
                phi_1319_ = _e473;
                if !(_e473) {
                    phi_1319_ = (_e461 > 1.0);
                }
                let _e477 = phi_1319_;
                phi_1327_ = _e477;
                if !(_e477) {
                    phi_1327_ = (_e455.z < -1.0);
                }
                let _e481 = phi_1327_;
                phi_1334_ = _e481;
                if !(_e481) {
                    phi_1334_ = (_e455.z > 1.0);
                }
                let _e485 = phi_1334_;
                if _e485 {
                    phi_2843_ = 1.0;
                } else {
                    let _e491 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e464.x, _e464.y), i32(_e464.z), _e455.z);
                    let _e497 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e464.x, _e464.y), i32(_e464.z), _e455.z, vec2<i32>(0, 1));
                    let _e504 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e464.x, _e464.y), i32(_e464.z), _e455.z, vec2<i32>(0, -1));
                    let _e511 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e464.x, _e464.y), i32(_e464.z), _e455.z, vec2<i32>(1, 0));
                    let _e518 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e464.x, _e464.y), i32(_e464.z), _e455.z, vec2<i32>(-1, 0));
                    phi_2843_ = (0.20000000298023224 * ((((_e491 + _e497) + _e504) + _e511) + _e518));
                }
                let _e522 = phi_2843_;
                let _e527 = unnamed_2.directional_lights[_e442].color;
                let _e529 = unnamed_2.directional_lights[_e442].direction;
                let _e532 = unnamed.uniforms.view;
                let _e542 = normalize((mat3x3<f32>(_e532[0].xyz, _e532[1].xyz, _e532[2].xyz) * -(_e529)));
                let _e544 = normalize((_e438 + _e542));
                let _e546 = abs(dot(_e423, _e438));
                let _e547 = (_e546 + 9.999999747378752e-6);
                let _e549 = clamp(dot(_e423, _e542), 0.0, 1.0);
                let _e551 = clamp(dot(_e423, _e544), 0.0, 1.0);
                let _e556 = (_e421 * _e421);
                let _e560 = ((((_e551 * _e556) - _e551) * _e551) + 1.0);
                local_3 = (_e440 + ((((_e419 * 0.31830987334251404) + (((_e425 + ((vec3<f32>(clamp(dot(_e425, vec3<f32>(16.5, 16.5, 16.5)), 0.0, 1.0)) - _e425) * pow((1.0 - clamp(dot(_e542, _e544), 0.0, 1.0)), 5.0))) * ((_e556 / ((3.1415927410125732 * _e560) * _e560)) * (0.5 / ((_e549 * sqrt((((((-9.999999747378752e-6 - _e546) * _e556) + _e547) * _e547) + _e556))) + (_e547 * sqrt(((((-(_e549) * _e556) + _e549) * _e549) + _e556))))))) * 1.0)) * _e527) * (_e549 * (_e522 * _e429))));
                continue;
            } else {
                break;
            }
            continuing {
                let _e664 = local_3;
                phi_2836_ = _e664;
                phi_2835_ = (_e442 + bitcast<u32>(1));
            }
        }
        let _e599 = local;
        let _e602 = local_1;
        let _e605 = local_2;
        let _e610 = unnamed.uniforms.ambient;
        o_color = max(vec4<f32>(_e599.x, _e602.y, _e605.z, _e164.w), (_e610 * _e164));
    }
    return;
}

@fragment 
fn main(@location(3) i_coords0_: vec2<f32>, @location(4) o_coords1: vec2<f32>, @location(5) i_color: vec4<f32>, @location(1) i_normal: vec3<f32>, @location(2) i_tangent: vec3<f32>, @location(0) i_view_position: vec4<f32>, @location(6) i_material: u32) -> @location(0) vec4<f32> {
    i_coords0_1 = i_coords0_;
    i_color_1 = i_color;
    i_normal_1 = i_normal;
    i_tangent_1 = i_tangent;
    i_view_position_1 = i_view_position;
    main_1();
    let _e11 = o_color;
    return _e11;
}
