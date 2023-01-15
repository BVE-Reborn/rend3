// -- DO NOT VALIDATE --

fn store_attribute_vec3_f32(byte_base_offset: u32, vertex_index: u32, value: vec3<f32>) {
    let first_element_idx = byte_base_offset / 4u + vertex_index * 3u;
    
    vertex_buffer[first_element_idx] = bitcast<u32>(value.x);
    vertex_buffer[first_element_idx + 1u] = bitcast<u32>(value.y);
    vertex_buffer[first_element_idx + 2u] = bitcast<u32>(value.z);
}
