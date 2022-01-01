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
    var phi_2410_: vec4<f32>;
    var phi_2408_: vec4<f32>;
    var phi_2412_: vec4<f32>;
    var phi_2411_: vec4<f32>;
    var phi_2413_: vec2<f32>;
    var phi_2414_: vec3<f32>;
    var phi_2415_: vec3<f32>;
    var phi_2416_: vec3<f32>;
    var phi_2548_: f32;
    var phi_2486_: f32;
    var phi_2436_: f32;
    var phi_1598_: bool;
    var phi_2417_: vec2<f32>;
    var phi_2489_: f32;
    var phi_2439_: f32;
    var phi_2550_: f32;
    var phi_2502_: f32;
    var phi_2452_: f32;
    var phi_2557_: f32;
    var phi_2551_: f32;
    var phi_2490_: f32;
    var phi_2440_: f32;
    var phi_2549_: f32;
    var phi_2487_: f32;
    var phi_2437_: f32;
    var phi_2547_: f32;
    var phi_2485_: f32;
    var phi_2435_: f32;
    var phi_2453_: f32;
    var phi_2510_: f32;
    var phi_2455_: f32;
    var phi_2458_: f32;
    var phi_2512_: f32;
    var phi_2481_: f32;
    var phi_2534_: f32;
    var phi_2513_: f32;
    var phi_2459_: f32;
    var phi_2511_: f32;
    var phi_2456_: f32;
    var phi_2509_: f32;
    var phi_2454_: f32;
    var phi_2535_: f32;
    var phi_2633_: vec3<f32>;
    var phi_2696_: vec3<f32>;
    var phi_2689_: f32;
    var phi_2665_: vec3<f32>;
    var phi_2637_: vec3<f32>;
    var phi_2626_: vec3<f32>;
    var phi_2536_: f32;
    var phi_2725_: vec3<f32>;
    var phi_2724_: u32;
    var phi_1236_: bool;
    var phi_1243_: bool;
    var phi_1250_: bool;
    var phi_1258_: bool;
    var phi_1265_: bool;
    var phi_2732_: f32;
    var local: vec3<f32>;
    var local_1: vec3<f32>;
    var local_2: vec3<f32>;
    var local_3: vec3<f32>;

    let _e94 = unnamed_1.material.uv_transform0_;
    let _e96 = unnamed_1.material.albedo;
    let _e98 = unnamed_1.material.emissive;
    let _e100 = unnamed_1.material.roughness;
    let _e102 = unnamed_1.material.metallic;
    let _e104 = unnamed_1.material.reflectance;
    let _e106 = unnamed_1.material.clear_coat;
    let _e108 = unnamed_1.material.clear_coat_roughness;
    let _e110 = unnamed_1.material.ambient_occlusion;
    let _e112 = unnamed_1.material.material_flags;
    let _e114 = unnamed_1.material.texture_enable;
    let _e115 = i_coords0_1;
    let _e119 = (_e94 * vec3<f32>(_e115.x, _e115.y, 1.0));
    let _e122 = vec2<f32>(_e119.x, _e119.y);
    let _e123 = dpdx(_e122);
    let _e124 = dpdy(_e122);
    if (((_e112 & 1u) != 0u)) {
        if ((((_e114 >> bitcast<u32>(0)) & 1u) != 0u)) {
            let _e131 = textureSampleGrad(albedo_tex, primary_sampler, _e122, _e123, _e124);
            phi_2410_ = _e131;
        } else {
            phi_2410_ = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
        let _e133 = phi_2410_;
        phi_2412_ = _e133;
        if (((_e112 & 2u) != 0u)) {
            let _e136 = i_color_1;
            phi_2408_ = _e136;
            if (((_e112 & 4u) != 0u)) {
                let _e139 = _e136.xyz;
                let _e147 = mix((_e139 * vec3<f32>(0.07739938050508499, 0.07739938050508499, 0.07739938050508499)), pow(((_e139 + vec3<f32>(0.054999999701976776, 0.054999999701976776, 0.054999999701976776)) * vec3<f32>(0.9478673338890076, 0.9478673338890076, 0.9478673338890076)), vec3<f32>(2.4000000953674316, 2.4000000953674316, 2.4000000953674316)), clamp(ceil((_e139 - vec3<f32>(0.040449999272823334, 0.040449999272823334, 0.040449999272823334))), vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0)));
                phi_2408_ = vec4<f32>(_e147.x, _e147.y, _e147.z, _e136.w);
            }
            let _e154 = phi_2408_;
            phi_2412_ = (_e133 * _e154);
        }
        let _e157 = phi_2412_;
        phi_2411_ = _e157;
    } else {
        phi_2411_ = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let _e159 = phi_2411_;
    let _e160 = (_e159 * _e96);
    if (((_e112 & 8192u) != 0u)) {
        let _e163 = i_normal_1;
        phi_2696_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2689_ = 0.0;
        phi_2665_ = normalize(_e163);
        phi_2637_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2626_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2536_ = 0.0;
    } else {
        if ((((_e114 >> bitcast<u32>(1)) & 1u) != 0u)) {
            let _e169 = textureSampleGrad(normal_tex, primary_sampler, _e122, _e123, _e124);
            if (((_e112 & 8u) != 0u)) {
                if (((_e112 & 16u) != 0u)) {
                    phi_2413_ = _e169.wy;
                } else {
                    phi_2413_ = _e169.xy;
                }
                let _e177 = phi_2413_;
                let _e179 = ((_e177 * 2.0) - vec2<f32>(1.0, 1.0));
                phi_2414_ = vec3<f32>(_e179.x, _e179.y, sqrt(((1.0 - (_e179.x * _e179.x)) - (_e179.y * _e179.y))));
            } else {
                phi_2414_ = normalize(((_e169.xyz * 2.0) - vec3<f32>(1.0, 1.0, 1.0)));
            }
            let _e193 = phi_2414_;
            phi_2415_ = _e193;
            if (((_e112 & 32u) != 0u)) {
                phi_2415_ = vec3<f32>(_e193.x, -(_e193.y), _e193.z);
            }
            let _e203 = phi_2415_;
            let _e204 = i_normal_1;
            let _e205 = normalize(_e204);
            let _e206 = i_tangent_1;
            let _e207 = normalize(_e206);
            phi_2416_ = (mat3x3<f32>(_e207, cross(_e205, _e207), _e205) * _e203);
        } else {
            let _e211 = i_normal_1;
            phi_2416_ = _e211;
        }
        let _e213 = phi_2416_;
        if (((_e112 & 64u) != 0u)) {
            if ((((_e114 >> bitcast<u32>(2)) & 1u) != 0u)) {
                let _e221 = textureSampleGrad(roughness_tex, primary_sampler, _e122, _e123, _e124);
                phi_2548_ = (_e110 * _e221.x);
                phi_2486_ = (_e100 * _e221.y);
                phi_2436_ = (_e102 * _e221.z);
            } else {
                phi_2548_ = _e110;
                phi_2486_ = _e100;
                phi_2436_ = _e102;
            }
            let _e229 = phi_2548_;
            let _e231 = phi_2486_;
            let _e233 = phi_2436_;
            phi_2547_ = _e229;
            phi_2485_ = _e231;
            phi_2435_ = _e233;
        } else {
            let _e235 = ((_e112 & 128u) != 0u);
            phi_1598_ = _e235;
            if (!(_e235)) {
                phi_1598_ = ((_e112 & 256u) != 0u);
            }
            let _e240 = phi_1598_;
            if (_e240) {
                if ((((_e114 >> bitcast<u32>(2)) & 1u) != 0u)) {
                    let _e245 = textureSampleGrad(roughness_tex, primary_sampler, _e122, _e123, _e124);
                    if (_e235) {
                        phi_2417_ = _e245.yz;
                    } else {
                        phi_2417_ = _e245.xy;
                    }
                    let _e249 = phi_2417_;
                    phi_2489_ = (_e100 * _e249.x);
                    phi_2439_ = (_e102 * _e249.y);
                } else {
                    phi_2489_ = _e100;
                    phi_2439_ = _e102;
                }
                let _e255 = phi_2489_;
                let _e257 = phi_2439_;
                if ((((_e114 >> bitcast<u32>(9)) & 1u) != 0u)) {
                    let _e262 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e122, _e123, _e124);
                    phi_2550_ = (_e110 * _e262.x);
                } else {
                    phi_2550_ = _e110;
                }
                let _e266 = phi_2550_;
                phi_2549_ = _e266;
                phi_2487_ = _e255;
                phi_2437_ = _e257;
            } else {
                phi_2551_ = 0.0;
                phi_2490_ = 0.0;
                phi_2440_ = 0.0;
                if (((_e112 & 512u) != 0u)) {
                    if ((((_e114 >> bitcast<u32>(2)) & 1u) != 0u)) {
                        let _e273 = textureSampleGrad(roughness_tex, primary_sampler, _e122, _e123, _e124);
                        phi_2502_ = (_e100 * _e273.x);
                    } else {
                        phi_2502_ = _e100;
                    }
                    let _e277 = phi_2502_;
                    if ((((_e114 >> bitcast<u32>(3)) & 1u) != 0u)) {
                        let _e282 = textureSampleGrad(metallic_tex, primary_sampler, _e122, _e123, _e124);
                        phi_2452_ = (_e102 * _e282.x);
                    } else {
                        phi_2452_ = _e102;
                    }
                    let _e286 = phi_2452_;
                    if ((((_e114 >> bitcast<u32>(9)) & 1u) != 0u)) {
                        let _e291 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e122, _e123, _e124);
                        phi_2557_ = (_e110 * _e291.x);
                    } else {
                        phi_2557_ = _e110;
                    }
                    let _e295 = phi_2557_;
                    phi_2551_ = _e295;
                    phi_2490_ = _e277;
                    phi_2440_ = _e286;
                }
                let _e297 = phi_2551_;
                let _e299 = phi_2490_;
                let _e301 = phi_2440_;
                phi_2549_ = _e297;
                phi_2487_ = _e299;
                phi_2437_ = _e301;
            }
            let _e303 = phi_2549_;
            let _e305 = phi_2487_;
            let _e307 = phi_2437_;
            phi_2547_ = _e303;
            phi_2485_ = _e305;
            phi_2435_ = _e307;
        }
        let _e309 = phi_2547_;
        let _e311 = phi_2485_;
        let _e313 = phi_2435_;
        if ((((_e114 >> bitcast<u32>(4)) & 1u) != 0u)) {
            let _e318 = textureSampleGrad(reflectance_tex, primary_sampler, _e122, _e123, _e124);
            phi_2453_ = (_e104 * _e318.x);
        } else {
            phi_2453_ = _e104;
        }
        let _e322 = phi_2453_;
        let _e323 = _e160.xyz;
        let _e324 = (1.0 - _e313);
        if (((_e112 & 1024u) != 0u)) {
            if ((((_e114 >> bitcast<u32>(5)) & 1u) != 0u)) {
                let _e338 = textureSampleGrad(clear_coat_tex, primary_sampler, _e122, _e123, _e124);
                phi_2510_ = (_e108 * _e338.y);
                phi_2455_ = (_e106 * _e338.x);
            } else {
                phi_2510_ = _e108;
                phi_2455_ = _e106;
            }
            let _e344 = phi_2510_;
            let _e346 = phi_2455_;
            phi_2509_ = _e344;
            phi_2454_ = _e346;
        } else {
            if (((_e112 & 2048u) != 0u)) {
                if ((((_e114 >> bitcast<u32>(5)) & 1u) != 0u)) {
                    let _e353 = textureSampleGrad(clear_coat_tex, primary_sampler, _e122, _e123, _e124);
                    phi_2458_ = (_e106 * _e353.x);
                } else {
                    phi_2458_ = _e106;
                }
                let _e357 = phi_2458_;
                if ((((_e114 >> bitcast<u32>(6)) & 1u) != 0u)) {
                    let _e362 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e122, _e123, _e124);
                    phi_2512_ = (_e108 * _e362.y);
                } else {
                    phi_2512_ = _e108;
                }
                let _e366 = phi_2512_;
                phi_2511_ = _e366;
                phi_2456_ = _e357;
            } else {
                phi_2513_ = 0.0;
                phi_2459_ = 0.0;
                if (((_e112 & 4096u) != 0u)) {
                    if ((((_e114 >> bitcast<u32>(5)) & 1u) != 0u)) {
                        let _e373 = textureSampleGrad(clear_coat_tex, primary_sampler, _e122, _e123, _e124);
                        phi_2481_ = (_e106 * _e373.x);
                    } else {
                        phi_2481_ = _e106;
                    }
                    let _e377 = phi_2481_;
                    if ((((_e114 >> bitcast<u32>(6)) & 1u) != 0u)) {
                        let _e382 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e122, _e123, _e124);
                        phi_2534_ = (_e108 * _e382.x);
                    } else {
                        phi_2534_ = _e108;
                    }
                    let _e386 = phi_2534_;
                    phi_2513_ = _e386;
                    phi_2459_ = _e377;
                }
                let _e388 = phi_2513_;
                let _e390 = phi_2459_;
                phi_2511_ = _e388;
                phi_2456_ = _e390;
            }
            let _e392 = phi_2511_;
            let _e394 = phi_2456_;
            phi_2509_ = _e392;
            phi_2454_ = _e394;
        }
        let _e396 = phi_2509_;
        let _e398 = phi_2454_;
        phi_2535_ = _e311;
        if ((_e398 != 0.0)) {
            phi_2535_ = mix(_e311, max(_e311, _e396), _e398);
        }
        let _e403 = phi_2535_;
        if ((((_e114 >> bitcast<u32>(7)) & 1u) != 0u)) {
            let _e409 = textureSampleGrad(emissive_tex, primary_sampler, _e122, _e123, _e124);
            phi_2633_ = (_e98 * _e409.xyz);
        } else {
            phi_2633_ = _e98;
        }
        let _e413 = phi_2633_;
        phi_2696_ = (_e323 * _e324);
        phi_2689_ = (_e403 * _e403);
        phi_2665_ = normalize(_e213);
        phi_2637_ = ((_e323 * _e313) + vec3<f32>((((0.1599999964237213 * _e322) * _e322) * _e324)));
        phi_2626_ = _e413;
        phi_2536_ = _e309;
    }
    let _e415 = phi_2696_;
    let _e417 = phi_2689_;
    let _e419 = phi_2665_;
    let _e421 = phi_2637_;
    let _e423 = phi_2626_;
    let _e425 = phi_2536_;
    let _e428 = unnamed_1.material.material_flags;
    if (((_e428 & 8192u) != 0u)) {
        o_color = _e160;
    } else {
        let _e431 = i_view_position_1;
        let _e434 = -(normalize(_e431.xyz));
        phi_2725_ = _e423;
        phi_2724_ = 0u;
        loop {
            let _e436 = phi_2725_;
            let _e438 = phi_2724_;
            let _e441 = unnamed_2.directional_light_header.total_lights;
            local = _e436;
            local_1 = _e436;
            local_2 = _e436;
            if ((_e438 < _e441)) {
                let _e446 = unnamed_2.directional_lights[_e438].view_proj;
                let _e449 = unnamed.uniforms.inv_view;
                let _e451 = ((_e446 * _e449) * _e431);
                let _e454 = ((_e451.xy * 0.5) + vec2<f32>(0.5, 0.5));
                let _e457 = (1.0 - _e454.y);
                let _e460 = vec4<f32>(_e454.x, _e457, f32(_e438), _e451.z);
                let _e461 = (_e454.x < 0.0);
                phi_1236_ = _e461;
                if (!(_e461)) {
                    phi_1236_ = (_e454.x > 1.0);
                }
                let _e465 = phi_1236_;
                phi_1243_ = _e465;
                if (!(_e465)) {
                    phi_1243_ = (_e457 < 0.0);
                }
                let _e469 = phi_1243_;
                phi_1250_ = _e469;
                if (!(_e469)) {
                    phi_1250_ = (_e457 > 1.0);
                }
                let _e473 = phi_1250_;
                phi_1258_ = _e473;
                if (!(_e473)) {
                    phi_1258_ = (_e451.z < -1.0);
                }
                let _e477 = phi_1258_;
                phi_1265_ = _e477;
                if (!(_e477)) {
                    phi_1265_ = (_e451.z > 1.0);
                }
                let _e481 = phi_1265_;
                if (_e481) {
                    phi_2732_ = 1.0;
                } else {
                    let _e487 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e460.x, _e460.y), i32(_e460.z), _e451.z);
                    phi_2732_ = _e487;
                }
                let _e489 = phi_2732_;
                let _e494 = unnamed_2.directional_lights[_e438].color;
                let _e496 = unnamed_2.directional_lights[_e438].direction;
                let _e499 = unnamed.uniforms.view;
                let _e509 = normalize((mat3x3<f32>(_e499[0].xyz, _e499[1].xyz, _e499[2].xyz) * -(_e496)));
                let _e511 = normalize((_e434 + _e509));
                let _e513 = abs(dot(_e419, _e434));
                let _e514 = (_e513 + 9.999999747378752e-6);
                let _e516 = clamp(dot(_e419, _e509), 0.0, 1.0);
                let _e518 = clamp(dot(_e419, _e511), 0.0, 1.0);
                let _e523 = (_e417 * _e417);
                let _e527 = ((((_e518 * _e523) - _e518) * _e518) + 1.0);
                local_3 = (_e436 + ((((_e415 * 0.31830987334251404) + (((_e421 + ((vec3<f32>(clamp(dot(_e421, vec3<f32>(16.5, 16.5, 16.5)), 0.0, 1.0)) - _e421) * pow((1.0 - clamp(dot(_e509, _e511), 0.0, 1.0)), 5.0))) * ((_e523 / ((3.1415927410125732 * _e527) * _e527)) * (0.5 / ((_e516 * sqrt((((((-9.999999747378752e-6 - _e513) * _e523) + _e514) * _e514) + _e523))) + (_e514 * sqrt(((((-(_e516) * _e523) + _e516) * _e516) + _e523))))))) * 1.0)) * _e494) * (_e516 * (_e489 * _e425))));
                continue;
            } else {
                break;
            }
            continuing {
                let _e631 = local_3;
                phi_2725_ = _e631;
                phi_2724_ = (_e438 + bitcast<u32>(1));
            }
        }
        let _e566 = local;
        let _e569 = local_1;
        let _e572 = local_2;
        let _e577 = unnamed.uniforms.ambient;
        o_color = max(vec4<f32>(_e566.x, _e569.y, _e572.z, _e160.w), (_e577 * _e160));
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
