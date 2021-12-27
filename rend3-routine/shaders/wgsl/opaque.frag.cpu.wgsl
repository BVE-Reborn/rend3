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
    var phi_2386_: vec4<f32>;
    var phi_2384_: vec4<f32>;
    var phi_2388_: vec4<f32>;
    var phi_2387_: vec4<f32>;
    var phi_2389_: vec2<f32>;
    var phi_2390_: vec3<f32>;
    var phi_2391_: vec3<f32>;
    var phi_2518_: f32;
    var phi_2458_: f32;
    var phi_2410_: f32;
    var phi_1577_: bool;
    var phi_2392_: vec2<f32>;
    var phi_2461_: f32;
    var phi_2413_: f32;
    var phi_2520_: f32;
    var phi_2473_: f32;
    var phi_2425_: f32;
    var phi_2526_: f32;
    var phi_2521_: f32;
    var phi_2462_: f32;
    var phi_2414_: f32;
    var phi_2519_: f32;
    var phi_2459_: f32;
    var phi_2411_: f32;
    var phi_2517_: f32;
    var phi_2457_: f32;
    var phi_2409_: f32;
    var phi_2426_: f32;
    var phi_2481_: f32;
    var phi_2428_: f32;
    var phi_2431_: f32;
    var phi_2483_: f32;
    var phi_2453_: f32;
    var phi_2504_: f32;
    var phi_2484_: f32;
    var phi_2432_: f32;
    var phi_2482_: f32;
    var phi_2429_: f32;
    var phi_2480_: f32;
    var phi_2427_: f32;
    var phi_2505_: f32;
    var phi_2601_: vec3<f32>;
    var phi_2664_: vec3<f32>;
    var phi_2657_: f32;
    var phi_2633_: vec3<f32>;
    var phi_2605_: vec3<f32>;
    var phi_2594_: vec3<f32>;
    var phi_2506_: f32;
    var phi_2693_: vec3<f32>;
    var phi_2692_: u32;
    var phi_1225_: bool;
    var phi_1232_: bool;
    var phi_1239_: bool;
    var phi_1247_: bool;
    var phi_1254_: bool;
    var phi_2700_: f32;
    var local: vec3<f32>;
    var local_1: vec3<f32>;
    var local_2: vec3<f32>;
    var local_3: vec3<f32>;

    let _e93 = unnamed_1.material.uv_transform0_;
    let _e95 = unnamed_1.material.albedo;
    let _e97 = unnamed_1.material.emissive;
    let _e99 = unnamed_1.material.roughness;
    let _e101 = unnamed_1.material.metallic;
    let _e103 = unnamed_1.material.reflectance;
    let _e105 = unnamed_1.material.clear_coat;
    let _e107 = unnamed_1.material.clear_coat_roughness;
    let _e109 = unnamed_1.material.ambient_occlusion;
    let _e111 = unnamed_1.material.material_flags;
    let _e113 = unnamed_1.material.texture_enable;
    let _e114 = i_coords0_1;
    let _e118 = (_e93 * vec3<f32>(_e114.x, _e114.y, 1.0));
    let _e121 = vec2<f32>(_e118.x, _e118.y);
    let _e122 = dpdx(_e121);
    let _e123 = dpdy(_e121);
    if (((_e111 & 1u) != 0u)) {
        if ((((_e113 >> bitcast<u32>(0)) & 1u) != 0u)) {
            let _e130 = textureSampleGrad(albedo_tex, primary_sampler, _e121, _e122, _e123);
            phi_2386_ = _e130;
        } else {
            phi_2386_ = vec4<f32>(1.0, 1.0, 1.0, 1.0);
        }
        let _e132 = phi_2386_;
        phi_2388_ = _e132;
        if (((_e111 & 2u) != 0u)) {
            let _e135 = i_color_1;
            phi_2384_ = _e135;
            if (((_e111 & 4u) != 0u)) {
                let _e138 = _e135.xyz;
                let _e146 = mix((_e138 * vec3<f32>(0.07739938050508499, 0.07739938050508499, 0.07739938050508499)), pow(((_e138 + vec3<f32>(0.054999999701976776, 0.054999999701976776, 0.054999999701976776)) * vec3<f32>(0.9478673338890076, 0.9478673338890076, 0.9478673338890076)), vec3<f32>(2.4000000953674316, 2.4000000953674316, 2.4000000953674316)), clamp(ceil((_e138 - vec3<f32>(0.040449999272823334, 0.040449999272823334, 0.040449999272823334))), vec3<f32>(0.0, 0.0, 0.0), vec3<f32>(1.0, 1.0, 1.0)));
                phi_2384_ = vec4<f32>(_e146.x, _e146.y, _e146.z, _e135.w);
            }
            let _e153 = phi_2384_;
            phi_2388_ = (_e132 * _e153);
        }
        let _e156 = phi_2388_;
        phi_2387_ = _e156;
    } else {
        phi_2387_ = vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let _e158 = phi_2387_;
    let _e159 = (_e158 * _e95);
    if (((_e111 & 4096u) != 0u)) {
        let _e162 = i_normal_1;
        phi_2664_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2657_ = 0.0;
        phi_2633_ = normalize(_e162);
        phi_2605_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2594_ = vec3<f32>(0.0, 0.0, 0.0);
        phi_2506_ = 0.0;
    } else {
        if ((((_e113 >> bitcast<u32>(1)) & 1u) != 0u)) {
            let _e168 = textureSampleGrad(normal_tex, primary_sampler, _e121, _e122, _e123);
            if (((_e111 & 8u) != 0u)) {
                if (((_e111 & 16u) != 0u)) {
                    phi_2389_ = _e168.wy;
                } else {
                    phi_2389_ = _e168.xy;
                }
                let _e176 = phi_2389_;
                let _e178 = ((_e176 * 2.0) - vec2<f32>(1.0, 1.0));
                phi_2390_ = vec3<f32>(_e178.x, _e178.y, sqrt(((1.0 - (_e178.x * _e178.x)) - (_e178.y * _e178.y))));
            } else {
                phi_2390_ = normalize(((_e168.xyz * 2.0) - vec3<f32>(1.0, 1.0, 1.0)));
            }
            let _e192 = phi_2390_;
            let _e193 = i_normal_1;
            let _e195 = i_tangent_1;
            phi_2391_ = (mat3x3<f32>(_e195, cross(normalize(_e193), normalize(_e195)), _e193) * _e192);
        } else {
            let _e200 = i_normal_1;
            phi_2391_ = _e200;
        }
        let _e202 = phi_2391_;
        if (((_e111 & 32u) != 0u)) {
            if ((((_e113 >> bitcast<u32>(2)) & 1u) != 0u)) {
                let _e210 = textureSampleGrad(roughness_tex, primary_sampler, _e121, _e122, _e123);
                phi_2518_ = (_e109 * _e210.x);
                phi_2458_ = (_e99 * _e210.z);
                phi_2410_ = (_e101 * _e210.y);
            } else {
                phi_2518_ = _e109;
                phi_2458_ = _e99;
                phi_2410_ = _e101;
            }
            let _e218 = phi_2518_;
            let _e220 = phi_2458_;
            let _e222 = phi_2410_;
            phi_2517_ = _e218;
            phi_2457_ = _e220;
            phi_2409_ = _e222;
        } else {
            let _e224 = ((_e111 & 64u) != 0u);
            phi_1577_ = _e224;
            if (!(_e224)) {
                phi_1577_ = ((_e111 & 128u) != 0u);
            }
            let _e229 = phi_1577_;
            if (_e229) {
                if ((((_e113 >> bitcast<u32>(2)) & 1u) != 0u)) {
                    let _e234 = textureSampleGrad(roughness_tex, primary_sampler, _e121, _e122, _e123);
                    if (_e224) {
                        phi_2392_ = _e234.yz;
                    } else {
                        phi_2392_ = _e234.xy;
                    }
                    let _e238 = phi_2392_;
                    phi_2461_ = (_e99 * _e238.y);
                    phi_2413_ = (_e101 * _e238.x);
                } else {
                    phi_2461_ = _e109;
                    phi_2413_ = _e101;
                }
                let _e244 = phi_2461_;
                let _e246 = phi_2413_;
                if ((((_e113 >> bitcast<u32>(9)) & 1u) != 0u)) {
                    let _e251 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e121, _e122, _e123);
                    phi_2520_ = (_e109 * _e251.x);
                } else {
                    phi_2520_ = _e109;
                }
                let _e255 = phi_2520_;
                phi_2519_ = _e255;
                phi_2459_ = _e244;
                phi_2411_ = _e246;
            } else {
                phi_2521_ = 0.0;
                phi_2462_ = 0.0;
                phi_2414_ = 0.0;
                if (((_e111 & 256u) != 0u)) {
                    if ((((_e113 >> bitcast<u32>(2)) & 1u) != 0u)) {
                        let _e262 = textureSampleGrad(roughness_tex, primary_sampler, _e121, _e122, _e123);
                        phi_2473_ = (_e99 * _e262.x);
                    } else {
                        phi_2473_ = _e99;
                    }
                    let _e266 = phi_2473_;
                    if ((((_e113 >> bitcast<u32>(3)) & 1u) != 0u)) {
                        let _e271 = textureSampleGrad(metallic_tex, primary_sampler, _e121, _e122, _e123);
                        phi_2425_ = (_e101 * _e271.x);
                    } else {
                        phi_2425_ = _e101;
                    }
                    let _e275 = phi_2425_;
                    if ((((_e113 >> bitcast<u32>(9)) & 1u) != 0u)) {
                        let _e280 = textureSampleGrad(ambient_occlusion_tex, primary_sampler, _e121, _e122, _e123);
                        phi_2526_ = (_e109 * _e280.x);
                    } else {
                        phi_2526_ = _e109;
                    }
                    let _e284 = phi_2526_;
                    phi_2521_ = _e284;
                    phi_2462_ = _e266;
                    phi_2414_ = _e275;
                }
                let _e286 = phi_2521_;
                let _e288 = phi_2462_;
                let _e290 = phi_2414_;
                phi_2519_ = _e286;
                phi_2459_ = _e288;
                phi_2411_ = _e290;
            }
            let _e292 = phi_2519_;
            let _e294 = phi_2459_;
            let _e296 = phi_2411_;
            phi_2517_ = _e292;
            phi_2457_ = _e294;
            phi_2409_ = _e296;
        }
        let _e298 = phi_2517_;
        let _e300 = phi_2457_;
        let _e302 = phi_2409_;
        if ((((_e113 >> bitcast<u32>(4)) & 1u) != 0u)) {
            let _e307 = textureSampleGrad(reflectance_tex, primary_sampler, _e121, _e122, _e123);
            phi_2426_ = (_e103 * _e307.x);
        } else {
            phi_2426_ = _e103;
        }
        let _e311 = phi_2426_;
        let _e312 = _e159.xyz;
        let _e313 = (1.0 - _e302);
        if (((_e111 & 512u) != 0u)) {
            if ((((_e113 >> bitcast<u32>(5)) & 1u) != 0u)) {
                let _e327 = textureSampleGrad(clear_coat_tex, primary_sampler, _e121, _e122, _e123);
                phi_2481_ = (_e107 * _e327.y);
                phi_2428_ = (_e105 * _e327.x);
            } else {
                phi_2481_ = _e107;
                phi_2428_ = _e105;
            }
            let _e333 = phi_2481_;
            let _e335 = phi_2428_;
            phi_2480_ = _e333;
            phi_2427_ = _e335;
        } else {
            if (((_e111 & 1024u) != 0u)) {
                if ((((_e113 >> bitcast<u32>(5)) & 1u) != 0u)) {
                    let _e342 = textureSampleGrad(clear_coat_tex, primary_sampler, _e121, _e122, _e123);
                    phi_2431_ = (_e105 * _e342.x);
                } else {
                    phi_2431_ = _e105;
                }
                let _e346 = phi_2431_;
                if ((((_e113 >> bitcast<u32>(6)) & 1u) != 0u)) {
                    let _e351 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e121, _e122, _e123);
                    phi_2483_ = (_e107 * _e351.y);
                } else {
                    phi_2483_ = _e107;
                }
                let _e355 = phi_2483_;
                phi_2482_ = _e355;
                phi_2429_ = _e346;
            } else {
                phi_2484_ = 0.0;
                phi_2432_ = 0.0;
                if (((_e111 & 2048u) != 0u)) {
                    if ((((_e113 >> bitcast<u32>(5)) & 1u) != 0u)) {
                        let _e362 = textureSampleGrad(clear_coat_tex, primary_sampler, _e121, _e122, _e123);
                        phi_2453_ = (_e105 * _e362.x);
                    } else {
                        phi_2453_ = _e105;
                    }
                    let _e366 = phi_2453_;
                    if ((((_e113 >> bitcast<u32>(6)) & 1u) != 0u)) {
                        let _e371 = textureSampleGrad(clear_coat_roughness_tex, primary_sampler, _e121, _e122, _e123);
                        phi_2504_ = (_e107 * _e371.x);
                    } else {
                        phi_2504_ = _e107;
                    }
                    let _e375 = phi_2504_;
                    phi_2484_ = _e375;
                    phi_2432_ = _e366;
                }
                let _e377 = phi_2484_;
                let _e379 = phi_2432_;
                phi_2482_ = _e377;
                phi_2429_ = _e379;
            }
            let _e381 = phi_2482_;
            let _e383 = phi_2429_;
            phi_2480_ = _e381;
            phi_2427_ = _e383;
        }
        let _e385 = phi_2480_;
        let _e387 = phi_2427_;
        phi_2505_ = _e300;
        if ((_e387 != 0.0)) {
            phi_2505_ = mix(_e300, max(_e300, _e385), _e387);
        }
        let _e392 = phi_2505_;
        if ((((_e113 >> bitcast<u32>(7)) & 1u) != 0u)) {
            let _e398 = textureSampleGrad(emissive_tex, primary_sampler, _e121, _e122, _e123);
            phi_2601_ = (_e97 * _e398.xyz);
        } else {
            phi_2601_ = _e97;
        }
        let _e402 = phi_2601_;
        phi_2664_ = (_e312 * _e313);
        phi_2657_ = (_e392 * _e392);
        phi_2633_ = normalize(_e202);
        phi_2605_ = ((_e312 * _e302) + vec3<f32>((((0.1599999964237213 * _e311) * _e311) * _e313)));
        phi_2594_ = _e402;
        phi_2506_ = _e298;
    }
    let _e404 = phi_2664_;
    let _e406 = phi_2657_;
    let _e408 = phi_2633_;
    let _e410 = phi_2605_;
    let _e412 = phi_2594_;
    let _e414 = phi_2506_;
    let _e417 = unnamed_1.material.material_flags;
    if (((_e417 & 4096u) != 0u)) {
        o_color = _e159;
    } else {
        let _e420 = i_view_position_1;
        let _e423 = -(normalize(_e420.xyz));
        phi_2693_ = _e412;
        phi_2692_ = 0u;
        loop {
            let _e425 = phi_2693_;
            let _e427 = phi_2692_;
            let _e430 = unnamed_2.directional_light_header.total_lights;
            local = _e425;
            local_1 = _e425;
            local_2 = _e425;
            if ((_e427 < _e430)) {
                let _e435 = unnamed_2.directional_lights[_e427].view_proj;
                let _e438 = unnamed.uniforms.inv_view;
                let _e440 = ((_e435 * _e438) * _e420);
                let _e443 = ((_e440.xy * 0.5) + vec2<f32>(0.5, 0.5));
                let _e446 = (1.0 - _e443.y);
                let _e449 = vec4<f32>(_e443.x, _e446, f32(_e427), _e440.z);
                let _e450 = (_e443.x < 0.0);
                phi_1225_ = _e450;
                if (!(_e450)) {
                    phi_1225_ = (_e443.x > 1.0);
                }
                let _e454 = phi_1225_;
                phi_1232_ = _e454;
                if (!(_e454)) {
                    phi_1232_ = (_e446 < 0.0);
                }
                let _e458 = phi_1232_;
                phi_1239_ = _e458;
                if (!(_e458)) {
                    phi_1239_ = (_e446 > 1.0);
                }
                let _e462 = phi_1239_;
                phi_1247_ = _e462;
                if (!(_e462)) {
                    phi_1247_ = (_e440.z < -1.0);
                }
                let _e466 = phi_1247_;
                phi_1254_ = _e466;
                if (!(_e466)) {
                    phi_1254_ = (_e440.z > 1.0);
                }
                let _e470 = phi_1254_;
                if (_e470) {
                    phi_2700_ = 1.0;
                } else {
                    let _e476 = textureSampleCompareLevel(shadow, shadow_sampler, vec2<f32>(_e449.x, _e449.y), i32(_e449.z), _e440.z);
                    phi_2700_ = _e476;
                }
                let _e478 = phi_2700_;
                let _e483 = unnamed_2.directional_lights[_e427].color;
                let _e485 = unnamed_2.directional_lights[_e427].direction;
                let _e488 = unnamed.uniforms.view;
                let _e498 = normalize((mat3x3<f32>(_e488[0].xyz, _e488[1].xyz, _e488[2].xyz) * -(_e485)));
                let _e500 = normalize((_e423 + _e498));
                let _e502 = abs(dot(_e408, _e423));
                let _e503 = (_e502 + 9.999999747378752e-6);
                let _e505 = clamp(dot(_e408, _e498), 0.0, 1.0);
                let _e507 = clamp(dot(_e408, _e500), 0.0, 1.0);
                let _e512 = (_e406 * _e406);
                let _e516 = ((((_e507 * _e512) - _e507) * _e507) + 1.0);
                local_3 = (_e425 + ((((_e404 * 0.31830987334251404) + (((_e410 + ((vec3<f32>(clamp(dot(_e410, vec3<f32>(16.5, 16.5, 16.5)), 0.0, 1.0)) - _e410) * pow((1.0 - clamp(dot(_e498, _e500), 0.0, 1.0)), 5.0))) * ((_e512 / ((3.1415927410125732 * _e516) * _e516)) * (0.5 / ((_e505 * sqrt((((((-9.999999747378752e-6 - _e502) * _e512) + _e503) * _e503) + _e512))) + (_e503 * sqrt(((((-(_e505) * _e512) + _e505) * _e505) + _e512))))))) * 1.0)) * _e483) * (_e505 * (_e478 * _e414))));
                continue;
            } else {
                break;
            }
            continuing {
                let _e619 = local_3;
                phi_2693_ = _e619;
                phi_2692_ = (_e427 + bitcast<u32>(1));
            }
        }
        let _e555 = local;
        let _e558 = local_1;
        let _e561 = local_2;
        let _e566 = unnamed.uniforms.ambient;
        o_color = max(vec4<f32>(_e555.x, _e558.y, _e561.z, _e159.w), (_e566 * _e159));
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
