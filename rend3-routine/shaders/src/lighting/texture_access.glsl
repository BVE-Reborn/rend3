#ifndef SHADER_TEXTURE_ACCESS_GLSL
#define SHADER_TEXTURE_ACCESS_GLSL

bool has_texture(uint idx) {
    return idx != 0;
}

#ifdef GPU_DRIVEN
#define MATERIAL_TYPE GPUMaterialData

#define HAS_ALBEDO_TEXTURE has_texture(material.albedo_tex)
#define HAS_NORMAL_TEXTURE has_texture(material.normal_tex)
#define HAS_ROUGHNESS_TEXTURE has_texture(material.roughness_tex)
#define HAS_METALLIC_TEXTURE has_texture(material.metallic_tex)
#define HAS_REFLECTANCE_TEXTURE has_texture(material.reflectance_tex)
#define HAS_CLEAR_COAT_TEXTURE has_texture(material.clear_coat_tex)
#define HAS_CLEAR_COAT_ROUGHNESS_TEXTURE has_texture(material.clear_coat_roughness_tex)
#define HAS_EMISSIVE_TEXTURE has_texture(material.emissive_tex)
#define HAS_ANISOTROPY_TEXTURE has_texture(material.anisotropy_tex)
#define HAS_AMBIENT_OCCLUSION_TEXTURE has_texture(material.ambient_occlusion_tex)

#define ALBEDO_TEXTURE textures[nonuniformEXT(material.albedo_tex - 1)]
#define NORMAL_TEXTURE textures[nonuniformEXT(material.normal_tex - 1)]
#define ROUGHNESS_TEXTURE textures[nonuniformEXT(material.roughness_tex - 1)]
#define METALLIC_TEXTURE textures[nonuniformEXT(material.metallic_tex - 1)]
#define REFLECTANCE_TEXTURE textures[nonuniformEXT(material.reflectance_tex - 1)]
#define CLEAR_COAT_TEXTURE textures[nonuniformEXT(material.clear_coat_tex - 1)]
#define CLEAR_COAT_ROUGHNESS_TEXTURE textures[nonuniformEXT(material.clear_coat_roughness_tex - 1)]
#define EMISSIVE_TEXTURE textures[nonuniformEXT(material.emissive_tex - 1)]
#define ANISOTROPY_TEXTURE textures[nonuniformEXT(material.anisotropy_tex - 1)]
#define AMBIENT_OCCLUSION_TEXTURE textures[nonuniformEXT(material.ambient_occlusion_tex - 1)]
#endif

#ifdef CPU_DRIVEN
#define MATERIAL_TYPE CPUMaterialData

#define HAS_ALBEDO_TEXTURE bool((material.texture_enable >> 0) & 0x1)
#define HAS_NORMAL_TEXTURE bool((material.texture_enable >> 1) & 0x1)
#define HAS_ROUGHNESS_TEXTURE bool((material.texture_enable >> 2) & 0x1)
#define HAS_METALLIC_TEXTURE bool((material.texture_enable >> 3) & 0x1)
#define HAS_REFLECTANCE_TEXTURE bool((material.texture_enable >> 4) & 0x1)
#define HAS_CLEAR_COAT_TEXTURE bool((material.texture_enable >> 5) & 0x1)
#define HAS_CLEAR_COAT_ROUGHNESS_TEXTURE bool((material.texture_enable >> 6) & 0x1)
#define HAS_EMISSIVE_TEXTURE bool((material.texture_enable >> 7) & 0x1)
#define HAS_ANISOTROPY_TEXTURE bool((material.texture_enable >> 8) & 0x1)
#define HAS_AMBIENT_OCCLUSION_TEXTURE bool((material.texture_enable >> 9) & 0x1)

#define ALBEDO_TEXTURE albedo_tex
#define NORMAL_TEXTURE normal_tex
#define ROUGHNESS_TEXTURE roughness_tex
#define METALLIC_TEXTURE metallic_tex
#define REFLECTANCE_TEXTURE reflectance_tex
#define CLEAR_COAT_TEXTURE clear_coat_tex
#define CLEAR_COAT_ROUGHNESS_TEXTURE clear_coat_roughness_tex
#define EMISSIVE_TEXTURE emissive_tex
#define ANISOTROPY_TEXTURE anisotropy_tex
#define AMBIENT_OCCLUSION_TEXTURE ambient_occlusion_tex
#endif

#endif // SHADER_TEXTURE_ACCESS_GLSL