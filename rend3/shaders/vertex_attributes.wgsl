// -- DO NOT VALIDATE --

struct Indices {
    object: u32,
    vertex: u32,
}

struct BatchIndices {
    /// Index _within_ the batch
    local_object: u32,
    /// Vertex index within the object
    vertex: u32,
}

let INVALID_VERTEX: u32 = 0x00FFFFFFu;

fn unpack_batch_index(vertex_index: u32) -> BatchIndices {
    return BatchIndices(
        vertex_index >> 24u,
        vertex_index & 0xFFFFFFu,
    );
}

fn pack_batch_index(local_object: u32, vertex: u32) -> u32 {
    return local_object << 24u | vertex & 0xFFFFFFu;
}

fn extract_attribute_vec2_f32(byte_base_offset: u32, vertex_index: u32) -> vec2<f32> {
    let first_element_idx = byte_base_offset / 4u + vertex_index * 2u;
    return vec2<f32>(
        bitcast<f32>(vertex_buffer[first_element_idx]),
        bitcast<f32>(vertex_buffer[first_element_idx + 1u]),
    );
}

fn extract_attribute_vec3_f32(byte_base_offset: u32, vertex_index: u32) -> vec3<f32> {
    let first_element_idx = byte_base_offset / 4u + vertex_index * 3u;
    return vec3<f32>(
        bitcast<f32>(vertex_buffer[first_element_idx]),
        bitcast<f32>(vertex_buffer[first_element_idx + 1u]),
        bitcast<f32>(vertex_buffer[first_element_idx + 2u]),
    );
}

fn extract_attribute_vec4_f32(byte_base_offset: u32, vertex_index: u32) -> vec4<f32> {
    let first_element_idx = byte_base_offset / 4u + vertex_index * 4u;
    return vec4<f32>(
        bitcast<f32>(vertex_buffer[first_element_idx]),
        bitcast<f32>(vertex_buffer[first_element_idx + 1u]),
        bitcast<f32>(vertex_buffer[first_element_idx + 2u]),
        bitcast<f32>(vertex_buffer[first_element_idx + 3u]),
    );
}

fn extract_attribute_vec4_u16(byte_base_offset: u32, vertex_index: u32) -> vec4<u32> {
    let first_element_idx = byte_base_offset / 4u + vertex_index * 2u;
    let value_0 = vertex_buffer[first_element_idx];
    let value_1 = vertex_buffer[first_element_idx + 1u];
    return vec4<u32>(
        value_0 & 0xFFFFu,
        (value_0 >> 16u) & 0xFFFFu,
        value_1 & 0xFFFFu,
        (value_1 >> 16u) & 0xFFFFu,
    );
}

fn extract_attribute_vec4_u8_unorm(byte_base_offset: u32, vertex_index: u32) -> vec4<f32> {
    let first_element_idx = byte_base_offset / 4u + vertex_index;
    return unpack4x8unorm(vertex_buffer[first_element_idx]);
}
