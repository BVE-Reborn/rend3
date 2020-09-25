struct ObjectInputData {
    uint start_idx;
    uint count;
    int vertex_offset;
    uint material_translation_idx;
    mat4 transform;
};

struct ObjectOutputData {
    mat4 model_view_proj;
    mat3 inv_trans_model_view;
    uint material_translation_idx;
};

struct IndirectCall {
    uint vertex_count;
    uint instance_count;
    uint base_index;
    int vertex_offset;
    uint base_instance;
};

struct MaterialData {
    uint color;
    uint normal;
    uint roughness;
    uint specular;
};

struct UniformData {
    mat4 view;
    mat4 view_proj;
};

