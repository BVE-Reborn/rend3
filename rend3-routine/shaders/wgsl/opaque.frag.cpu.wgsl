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
[[group(2), binding(1)]]
var albedo_tex: texture_2d<f32>;
var<private> i_color_1: vec4<f32>;
var<private> i_normal_1: vec3<f32>;
[[group(2), binding(2)]]
var normal_tex: texture_2d<f32>;
var<private> i_tangent_1: vec3<f32>;
[[group(2), binding(3)]]
var roughness_tex: texture_2d<f32>;
[[group(2), binding(10)]]
var ambient_occlusion_tex: texture_2d<f32>;
[[group(2), binding(4)]]
var metallic_tex: texture_2d<f32>;
[[group(2), binding(5)]]
var reflectance_tex: texture_2d<f32>;
[[group(2), binding(6)]]
var clear_coat_tex: texture_2d<f32>;
[[group(2), binding(7)]]
var clear_coat_roughness_tex: texture_2d<f32>;
[[group(2), binding(8)]]
var emissive_tex: texture_2d<f32>;
[[group(0), binding(0)]]
var primary_sampler: sampler;
[[group(0), binding(3)]]
var<uniform> unnamed: UniformBuffer;
[[group(2), binding(0)]]
var<storage> unnamed_1: TextureData;
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
    var phi_2520_: vec4<f32>;
    var phi_2518_: vec4<f32>;
    var phi_2522_: vec4<f32>;
    var phi_2521_: vec4<f32>;
    var phi_2523_: vec2<f32>;
    var phi_2524_: vec3<f32>;
    var phi_2525_: vec3<f32>;
    var phi_2526_: vec3<f32>;
    var phi_2658_: f32;
    var phi_2596_: f32;
    var phi_2546_: f32;
    var phi_1659_: bool;
    var phi_2527_: vec2<f32>;
    var phi_2599_: f32;
    var phi_2549_: f32;
    var phi_2660_: f32;
    var phi_2612_: f32;
    var phi_2562_: f32;
    var phi_2667_: f32;
    var phi_2661_: f32;
    var phi_2600_: f32;
    var phi_2550_: f32;
    var phi_2659_: f32;
    var phi_2597_: f32;
    var phi_2547_: f32;
    var phi_2657_: f32;
    var phi_2595_: f32;
    var phi_2545_: f32;
    var phi_2563_: f32;
    var phi_2620_: f32;
    var phi_2565_: f32;
    var phi_2568_: f32;
    var phi_2622_: f32;
    var phi_2591_: f32;
    var phi_2644_: f32;
    var phi_2623_: f32;
    var phi_2569_: f32;
    var phi_2621_: f32;
    var phi_2566_: f32;
    var phi_2619_: f32;
    var phi_2564_: f32;
    var phi_2645_: f32;
    var phi_2743_: vec3<f32>;
    var phi_2806_: vec3<f32>;
    var phi_2799_: f32;
    var phi_2775_: vec3<f32>;
    var phi_2747_: vec3<f32>;
    var phi_2736_: vec3<f32>;
    var phi_2646_: f32;
    var phi_2835_: vec3<f32>;
    var phi_2834_: u32;
    var phi_1304_: bool;
    var phi_1311_: bool;
    var phi_1318_: bool;
    var phi_1326_: bool;
    var phi_1333_: bool;
    var phi_2842_: f32;
    var local: vec3<f32>;
    var local_1: vec3<f32>;
    var local_2: vec3<f32>;
    var local_3: vec3<f32>;

    let _e100 = unnamed_1.material.uv_transform0_;
    let _e102 = unnamed_1.material.albedo;
    let _e104 = unnamed_1.material.emissive;
    let _e106 = unnamed_1.material.roughness;
    let _e108 = unnamed_1.material.metallic;
    let _e110 = unnamed_1.material.reflectance;
    let _e112 = unnamed_1.material.clear_coat;
    let _e114 = unnamed_1.material.clear_coat_roughness;
    let _e116 = unnamed_1.material.ambient_occlusion;
    let _e118 = unnamed_1.material.material_flags;
    let _e120 = unnamed_1.material.texture_enable;
    let _e121 = i_coords0_1;
    let _e125 = (_e100 * vec3<f32>(_e121.x, _e121.y, 1.0));
    let _e128 = vec2<f32>(_e125.x, _e125.y);
    let _e129 = dpdx(_e128);
    let _e130 = dpdy(_e128);
    if (((_e118 & 1u) != 0u)) {
        if ((((_e120 >> bitcast<u32>(0)) & 1u) != 0u)) {
            let _e137 = textureSampleGrad(albedo_tex, primary_sampler, _e128, _e129, _e130);
            phi_2520_ = _e137;
        } else {
            phi_2520_ = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
        let _e139 = phi_2520_;
        phi_2522_ = _e139;
        if (((_e118 & 2u) != 0u)) {
            let _e142 = i_color_1;
            phi_2518_ = _e142;
            if (((_e118 & 4u) != 0u)) {
                let _e145 = _e142.xyz;
                let _e153 = mix((_e145 * vec3<f32>(0.07739938050508499, 0.07739938050508499, 0.07739938050508499)), pow(((_e145 + vec3<f32>(0.054999999701976776, 0.054999999701976776, 0.054999999701976776)) * vec3<f32>(0.9478673338890076, 0.9478673338890076, 0.9478673338890076)), vec3<f32>(2.4000000953674316, 2.4000000953674316, 2.4000000953674316)), clamp(ceil((_e145 - vec3<f32>(0.040449999272823334, 0.040449999272823334, 0.040449999272823334))), vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0)));
                phi_2518_ = vec4<f32>(_e153.x, _e153.y, _e153.z, _e142.w);
            }
            let _e160 = phi_2518_;
            phi_2522_ = (_e139 * _e160);
        }
        let _e163 = phi_2522_;
        phi_2521_ = _e163;
    } else {
        phi_2521_ = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let _e165 = phi_2521_;
    let _e166 = (_e165 * _e102);
    if (((_e118 & 8192u) != 0u)) {
        let _e169 = i_normal_1;
        phi_2806_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2799_ = 0.0;
        phi_2775_ = normalize(_e169);
        phi_2747_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2736_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2646_ = 0.0;
    } else {
        if ((((_e120 >> bitcast<u32>(1)) & 1u) != 0u)) {
            let _e175 = textureSampleGrad(normal_tex, primary_sampler, _e128, _e129, _e130);
            if (((_e118 & 8u) != 0u)) {
                if (((_e118 & 16u) != 0u)) {
                    phi_2523_ = _e175.wy;
                } else {
                    phi_2523_ = _e175.xy;
                }
                let _e183 = phi_2523_;
                let _e185 = ((_e183 * 2.0) - vec2<f32>(1.0, 1.0));
                phi_2524_ = vec3<f32>(_e185.x, _e185.y, sqrt(((1.0 - (_e185.x * _e185.x)) - (_e185.y * _e185.y))));
            } else {
                phi_2524_ = normalize(((_e175.xyz * 2.0) - vec3<f32>(1.0, 1.0, 1.0)));
            }
            let _e199 = phi_2524_;
            phi_2525_ = _e199;
            if (((_e118 & 32u) != 0u)) {
                phi_2525_ = vec3<f32>(_e199.x, -(_e199.y), _e199.z);
            }
            let _e209 = phi_2525_;
            let _e210 = i_normal_1;
            let _e211 = normalize(_e210);
            let _e212 = i_tangent_1;
            let _e213 = normalize(_e212);
            phi_2526_ = (mat3x3<f32>(_e213, cross(_e211, _e213), _e211) * _e209);
        } else {
            let _e217 = i_normal_1;
            phi_2526_ = _e217;
        }
        let _e219 = phi_2526_;
        if (((_e118 & 64u) != 0u)) {
            if ((((_e120 >> bitcast<u32>(2)) & 1u) != 0u)) {
                let _e227 = textureSampleGrad(roughness_tex, primary_sampler, _e128, _e129, _e130);
                phi_2658_ = (_e116 * _e227.x);
                phi_2596_ = (_e106 * _e227.y);
                phi_2546_ = (_e108 * _e227.z);
            } else {
                phi_2658_ = _e116;
                phi_2596_ = _e106;
                phi_2546_ = _e108;
            }
            let _e235 = phi_2658_;
            let _e237 = phi_2596_;
            let _e239 = phi_2546_;
            phi_2657_ = _e235;
            phi_2595_ = _e237;
            phi_2545_ = _e239;
        } else {
            let _e241 = ((_e118 & 128u) != 0u);
            phi_1659_ = _e241;
            if (!(_e241)) {
                phi_1659_ = ((_e118 & 256u) != 0u);
            }
            let _e246 = phi_1659_;
            if (_e246) {
                if ((((_e120 >> bitcast<u32>(2)) & 1u) != 0u)) {
                    let _e251 = textureSampleGrad(roughness_tex, primary_sampler, _e128, _e129, _e130);
                    if (_e241) {
                        phi_2527_ = _e251.yz;
                    } else {
                        phi_2527_ = _e251.xy;
                    }
                    let _e255 = phi_2527_;
                    phi_2599_ = (_e106 * _e255.x);
                    phi_2549_ = (_e108 * _e255.y);
                } else {
                    phi_2599_ = _e106;
                    phi_2549_ = _e108;
                }
                let _e261 = phi_2599_;
                let _e263 = phi_2549_;
                if ((((_e120 >> bitcast<u32>(9)) & 1u) != 0u)) {
                    let _e268 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e128, _e129, _e130);
                    phi_2660_ = (_e116 * _e268.x);
                } else {
                    phi_2660_ = _e116;
                }
                let _e272 = phi_2660_;
                phi_2659_ = _e272;
                phi_2597_ = _e261;
                phi_2547_ = _e263;
            } else {
                phi_2661_ = 0.0;
                phi_2600_ = 0.0;
                phi_2550_ = 0.0;
                if (((_e118 & 512u) != 0u)) {
                    if ((((_e120 >> bitcast<u32>(2)) & 1u) != 0u)) {
                        let _e279 = textureSampleGrad(roughness_tex, primary_sampler, _e128, _e129, _e130);
                        phi_2612_ = (_e106 * _e279.x);
                    } else {
                        phi_2612_ = _e106;
                    }
                    let _e283 = phi_2612_;
                    if ((((_e120 >> bitcast<u32>(3)) & 1u) != 0u)) {
                        let _e288 = textureSampleGrad(metallic_tex, primary_sampler, _e128, _e129, _e130);
                        phi_2562_ = (_e108 * _e288.x);
                    } else {
                        phi_2562_ = _e108;
                    }
                    let _e292 = phi_2562_;
                    if ((((_e120 >> bitcast<u32>(9)) & 1u) != 0u)) {
                        let _e297 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e128, _e129, _e130);
                        phi_2667_ = (_e116 * _e297.x);
                    } else {
                        phi_2667_ = _e116;
                    }
                    let _e301 = phi_2667_;
                    phi_2661_ = _e301;
                    phi_2600_ = _e283;
                    phi_2550_ = _e292;
                }
                let _e303 = phi_2661_;
                let _e305 = phi_2600_;
                let _e307 = phi_2550_;
                phi_2659_ = _e303;
                phi_2597_ = _e305;
                phi_2547_ = _e307;
            }
            let _e309 = phi_2659_;
            let _e311 = phi_2597_;
            let _e313 = phi_2547_;
            phi_2657_ = _e309;
            phi_2595_ = _e311;
            phi_2545_ = _e313;
        }
        let _e315 = phi_2657_;
        let _e317 = phi_2595_;
        let _e319 = phi_2545_;
        if ((((_e120 >> bitcast<u32>(4)) & 1u) != 0u)) {
            let _e324 = textureSampleGrad(reflectance_tex, primary_sampler, _e128, _e129, _e130);
            phi_2563_ = (_e110 * _e324.x);
        } else {
            phi_2563_ = _e110;
        }
        let _e328 = phi_2563_;
        let _e329 = _e166.xyz;
        let _e330 = (1.0 - _e319);
        if (((_e118 & 1024u) != 0u)) {
            if ((((_e120 >> bitcast<u32>(5)) & 1u) != 0u)) {
                let _e344 = textureSampleGrad(clear_coat_tex, primary_sampler, _e128, _e129, _e130);
                phi_2620_ = (_e114 * _e344.y);
                phi_2565_ = (_e112 * _e344.x);
            } else {
                phi_2620_ = _e114;
                phi_2565_ = _e112;
            }
            let _e350 = phi_2620_;
            let _e352 = phi_2565_;
            phi_2619_ = _e350;
            phi_2564_ = _e352;
        } else {
            if (((_e118 & 2048u) != 0u)) {
                if ((((_e120 >> bitcast<u32>(5)) & 1u) != 0u)) {
                    let _e359 = textureSampleGrad(clear_coat_tex, primary_sampler, _e128, _e129, _e130);
                    phi_2568_ = (_e112 * _e359.x);
                } else {
                    phi_2568_ = _e112;
                }
                let _e363 = phi_2568_;
                if ((((_e120 >> bitcast<u32>(6)) & 1u) != 0u)) {
                    let _e368 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e128, _e129, _e130);
                    phi_2622_ = (_e114 * _e368.y);
                } else {
                    phi_2622_ = _e114;
                }
                let _e372 = phi_2622_;
                phi_2621_ = _e372;
                phi_2566_ = _e363;
            } else {
                phi_2623_ = 0.0;
                phi_2569_ = 0.0;
                if (((_e118 & 4096u) != 0u)) {
                    if ((((_e120 >> bitcast<u32>(5)) & 1u) != 0u)) {
                        let _e379 = textureSampleGrad(clear_coat_tex, primary_sampler, _e128, _e129, _e130);
                        phi_2591_ = (_e112 * _e379.x);
                    } else {
                        phi_2591_ = _e112;
                    }
                    let _e383 = phi_2591_;
                    if ((((_e120 >> bitcast<u32>(6)) & 1u) != 0u)) {
                        let _e388 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e128, _e129, _e130);
                        phi_2644_ = (_e114 * _e388.x);
                    } else {
                        phi_2644_ = _e114;
                    }
                    let _e392 = phi_2644_;
                    phi_2623_ = _e392;
                    phi_2569_ = _e383;
                }
                let _e394 = phi_2623_;
                let _e396 = phi_2569_;
                phi_2621_ = _e394;
                phi_2566_ = _e396;
            }
            let _e398 = phi_2621_;
            let _e400 = phi_2566_;
            phi_2619_ = _e398;
            phi_2564_ = _e400;
        }
        let _e402 = phi_2619_;
        let _e404 = phi_2564_;
        phi_2645_ = _e317;
        if ((_e404 != 0.0)) {
            phi_2645_ = mix(_e317, max(_e317, _e402), _e404);
        }
        let _e409 = phi_2645_;
        if ((((_e120 >> bitcast<u32>(7)) & 1u) != 0u)) {
            let _e415 = textureSampleGrad(emissive_tex, primary_sampler, _e128, _e129, _e130);
            phi_2743_ = (_e104 * _e415.xyz);
        } else {
            phi_2743_ = _e104;
        }
        let _e419 = phi_2743_;
        phi_2806_ = (_e329 * _e330);
        phi_2799_ = (_e409 * _e409);
        phi_2775_ = normalize(_e219);
        phi_2747_ = ((_e329 * _e319) + vec3<f32>((((0.1599999964237213 * _e328) * _e328) * _e330)));
        phi_2736_ = _e419;
        phi_2646_ = _e315;
    }
    let _e421 = phi_2806_;
    let _e423 = phi_2799_;
    let _e425 = phi_2775_;
    let _e427 = phi_2747_;
    let _e429 = phi_2736_;
    let _e431 = phi_2646_;
    let _e434 = unnamed_1.material.material_flags;
    if (((_e434 & 8192u) != 0u)) {
        o_color = _e166;
    } else {
        let _e437 = i_view_position_1;
        let _e440 = -(normalize(_e437.xyz));
        phi_2835_ = _e429;
        phi_2834_ = 0u;
        loop {
            let _e442 = phi_2835_;
            let _e444 = phi_2834_;
            let _e447 = unnamed_2.directional_light_header.total_lights;
            local = _e442;
            local_1 = _e442;
            local_2 = _e442;
            if ((_e444 < _e447)) {
                let _e452 = unnamed_2.directional_lights[_e444].view_proj;
                let _e455 = unnamed.uniforms.inv_view;
                let _e457 = ((_e452 * _e455) * _e437);
                let _e460 = ((_e457.xy * 0.5) + vec2<f32>(0.5, 0.5));
                let _e463 = (1.0 - _e460.y);
                let _e466 = vec4<f32>(_e460.x, _e463, f32(_e444), _e457.z);
                let _e467 = (_e460.x < 0.0);
                phi_1304_ = _e467;
                if (!(_e467)) {
                    phi_1304_ = (_e460.x > 1.0);
                }
                let _e471 = phi_1304_;
                phi_1311_ = _e471;
                if (!(_e471)) {
                    phi_1311_ = (_e463 < 0.0);
                }
                let _e475 = phi_1311_;
                phi_1318_ = _e475;
                if (!(_e475)) {
                    phi_1318_ = (_e463 > 1.0);
                }
                let _e479 = phi_1318_;
                phi_1326_ = _e479;
                if (!(_e479)) {
                    phi_1326_ = (_e457.z < -1.0);
                }
                let _e483 = phi_1326_;
                phi_1333_ = _e483;
                if (!(_e483)) {
                    phi_1333_ = (_e457.z > 1.0);
                }
                let _e487 = phi_1333_;
                if (_e487) {
                    phi_2842_ = 1.0;
                } else {
                    let _e493 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e466.x, _e466.y), i32(_e466.z), _e457.z);
                    let _e499 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e466.x, _e466.y), i32(_e466.z), _e457.z, vec2<i32>(0, 1));
                    let _e506 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e466.x, _e466.y), i32(_e466.z), _e457.z, vec2<i32>(0, -1));
                    let _e513 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e466.x, _e466.y), i32(_e466.z), _e457.z, vec2<i32>(1, 0));
                    let _e520 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e466.x, _e466.y), i32(_e466.z), _e457.z, vec2<i32>(-1, 0));
                    phi_2842_ = (0.20000000298023224 * ((((_e493 + _e499) + _e506) + _e513) + _e520));
                }
                let _e524 = phi_2842_;
                let _e529 = unnamed_2.directional_lights[_e444].color;
                let _e531 = unnamed_2.directional_lights[_e444].direction;
                let _e534 = unnamed.uniforms.view;
                let _e544 = normalize((mat3x3<f32>(_e534[0].xyz, _e534[1].xyz, _e534[2].xyz) * -(_e531)));
                let _e546 = normalize((_e440 + _e544));
                let _e548 = abs(dot(_e425, _e440));
                let _e549 = (_e548 + 9.999999747378752e-6);
                let _e551 = clamp(dot(_e425, _e544), 0.0, 1.0);
                let _e553 = clamp(dot(_e425, _e546), 0.0, 1.0);
                let _e558 = (_e423 * _e423);
                let _e562 = ((((_e553 * _e558) - _e553) * _e553) + 1.0);
                local_3 = (_e442 + ((((_e421 * 0.31830987334251404) + (((_e427 + ((vec3<f32>(clamp(dot(_e427, vec3<f32>(16.5, 16.5, 16.5)), 0.0, 1.0)) - _e427) * pow((1.0 - clamp(dot(_e544, _e546), 0.0, 1.0)), 5.0))) * ((_e558 / ((3.1415927410125732 * _e562) * _e562)) * (0.5 / ((_e551 * sqrt((((((-9.999999747378752e-6 - _e548) * _e558) + _e549) * _e549) + _e558))) + (_e549 * sqrt(((((-(_e551) * _e558) + _e551) * _e551) + _e558))))))) * 1.0)) * _e529) * (_e551 * (_e524 * _e431))));
                continue;
            } else {
                break;
            }
            continuing {
                let _e666 = local_3;
                phi_2835_ = _e666;
                phi_2834_ = (_e444 + bitcast<u32>(1));
            }
        }
        let _e601 = local;
        let _e604 = local_1;
        let _e607 = local_2;
        let _e612 = unnamed.uniforms.ambient;
        o_color = max(vec4<f32>(_e601.x, _e604.y, _e607.z, _e166.w), (_e612 * _e166));
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
