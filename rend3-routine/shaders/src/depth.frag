#version 440

#ifdef GPU_DRIVEN
#extension GL_EXT_nonuniform_qualifier : require
#endif

layout(location = 0) in vec4 i_position;
layout(location = 1) in vec2 i_coords0;
layout(location = 2) in vec4 i_color;
layout(location = 3) flat in uint i_material;

#ifdef ALPHA_CUTOUT

layout(set = 0, binding = 0) uniform sampler primary_sampler;
#ifdef GPU_DRIVEN
layout(set = 1, binding = 1, std430) readonly buffer MaterialBuffer {
    float material_data[];
};
layout(set = 3, binding = 0) uniform texture2D textures[];
#endif
#ifdef CPU_DRIVEN
layout(set = 3, binding = 0, std430) readonly buffer TextureData {
    float material_data[];
};
layout(set = 3, binding = 1) uniform texture2D texture;
#endif
layout(set = 2, binding = 0) uniform DataAbi {
    uint stride; // Stride in offset into a float array (i.e. byte index / 4). Unused when GpuDriven.
    uint texture_offset; // Must be zero when GpuDriven. When GpuDriven, it's the index into the material data with the texture enable bitflag.
    uint cutoff_offset; // Stride in offset into a float array  (i.e. byte index / 4)
    uint uv_transform_offset; // Stride in offset into a float array pointing to a mat3 with the uv transform (i.e. byte index / 4). 0xFFFFFFFF represents "no transform"
};

void main() {
    uint base_material_offset = stride * i_material;
    float cutoff = material_data[base_material_offset + cutoff_offset];

    vec2 coords;
    if (uv_transform_offset != 0xFFFFFFFF) {
        uint base_transform_offset = base_material_offset + uv_transform_offset;
        mat3 transform = mat3(
            material_data[base_transform_offset + 0],
            material_data[base_transform_offset + 1],
            material_data[base_transform_offset + 2],
            material_data[base_transform_offset + 4],
            material_data[base_transform_offset + 5],
            material_data[base_transform_offset + 6],
            material_data[base_transform_offset + 8],
            material_data[base_transform_offset + 9],
            material_data[base_transform_offset + 10]
        );
        coords = vec2(transform * vec3(i_coords0, 1.0));
    } else {
        coords = i_coords0;
    }
    vec2 uvdx = dFdx(coords);
    vec2 uvdy = dFdy(coords);

    #ifdef GPU_DRIVEN
    uint texture_index = floatBitsToUint(material_data[base_material_offset + texture_offset]);
    if (texture_index != 0) {
        float alpha = textureGrad(sampler2D(textures[nonuniformEXT(texture_index - 1)], primary_sampler), coords, uvdx, uvdy).a;

        if (alpha <= cutoff) {
            discard;
        }
    }
    #endif
    #ifdef CPU_DRIVEN
    uint texture_enable_bitflags = floatBitsToUint(material_data[base_material_offset + texture_offset]);
    if (bool(texture_enable_bitflags & 0x1)) {
        float alpha = textureGrad(sampler2D(texture, primary_sampler), coords, uvdx, uvdy).a;

        if (alpha <= cutoff) {
            discard;
        }
    }
    #endif
}
#else // ALPHA_CUTOUT
void main() {}
#endif // ALPHA_CUTOUT
