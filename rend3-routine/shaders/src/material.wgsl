const FLAGS_ALBEDO_ACTIVE: u32        = 0x0001u;
const FLAGS_ALBEDO_BLEND: u32         = 0x0002u;
const FLAGS_ALBEDO_VERTEX_SRGB: u32   = 0x0004u;
const FLAGS_BICOMPONENT_NORMAL: u32   = 0x0008u;
const FLAGS_SWIZZLED_NORMAL: u32      = 0x0010u;
const FLAGS_YDOWN_NORMAL: u32         = 0x0020u;
const FLAGS_AOMR_COMBINED: u32        = 0x0040u;
const FLAGS_AOMR_SWIZZLED_SPLIT: u32  = 0x0080u;
const FLAGS_AOMR_SPLIT: u32           = 0x0100u;
const FLAGS_AOMR_BW_SPLIT: u32        = 0x0200u;
const FLAGS_CC_GLTF_COMBINED: u32     = 0x0400u;
const FLAGS_CC_GLTF_SPLIT: u32        = 0x0800u;
const FLAGS_CC_BW_SPLIT: u32          = 0x1000u;
const FLAGS_UNLIT: u32                = 0x2000u;
const FLAGS_NEAREST: u32              = 0x4000u;

fn extract_material_flag(data: u32, flag: u32) -> bool {
    return bool(data & flag);
}

struct GpuMaterialData {
    albedo_tex: u32,
    normal_tex: u32,
    roughness_tex: u32,
    metallic_tex: u32,
    // -- 16 --
    reflectance_tex: u32,
    clear_coat_tex: u32,
    clear_coat_roughness_tex: u32,
    emissive_tex: u32,
    // -- 16 --
    anisotropy_tex: u32,
    ambient_occlusion_tex: u32,
    _padding0: u32,
    _padding1: u32,
    
    // -- 16 --

    uv_transform0: mat3x3<f32>,
    // -- 16 --
    uv_transform1: mat3x3<f32>,
    // -- 16 --
    albedo: vec4<f32>,
    // -- 16 --
    emissive: vec3<f32>,
    roughness: f32,
    // -- 16 --
    metallic: f32,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    // -- 16 --
    anisotropy: f32,
    ambient_occlusion: f32,
    alpha_cutout: f32,
    flags: u32,
}

struct CpuMaterialData {
    uv_transform0: mat3x3<f32>,
    // -- 16 --
    uv_transform1: mat3x3<f32>,
    // -- 16 --
    albedo: vec4<f32>,
    // -- 16 --
    emissive: vec3<f32>,
    roughness: f32,
    // -- 16 --
    metallic: f32,
    reflectance: f32,
    clear_coat: f32,
    clear_coat_roughness: f32,
    // -- 16 --
    anisotropy: f32,
    ambient_occlusion: f32,
    alpha_cutout: f32,
    flags: u32,
    
    // -- 16 --
    texture_enable: u32,
};
