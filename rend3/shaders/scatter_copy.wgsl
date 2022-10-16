// Generic data scatter compute shader.
//
// Takes a sparse self-describing copy operation
// and executes the copy into the given destination buffer.
//
// This allows us to do sparse cpu buffer -> gpu buffer copies at
// the expense of 4 bytes per value.

struct TransferSource {
    // Count of 4-byte words of data to copy.
    words_to_copy: u32,
    // Count of structures (_not_ words) to copy.
    count: u32,

    // Really an array of the following structures `(words_to_copy + 4) * 4` bytes apart.
    // 
    // {
    //     // Offset in the destination buffer in 4-byte words.
    //     destination_word_offset: u32,
    //     data_word_0: u32,
    //     ..,
    //     data_word_N: u32,
    // }
    data: array<u32>,
}

@group(0) @binding(0)
var<storage, read> transfer_source: TransferSource;
@group(0) @binding(1)
var<storage, read_write> transfer_destination: array<u32>;

@compute @workgroup_size(64)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    // Each invocation copies a whole T, which can be any size, though it's presumed small.
    let index = gid.x;
    if (index >= transfer_source.count) {
        return;
    }

    let words_to_copy = transfer_source.words_to_copy;
    let stride = words_to_copy + 1u;

    let struct_word_offset = index * stride;

    let destination_word_offset = transfer_source.data[struct_word_offset];
    let data_word_offset = struct_word_offset + 1u;
    
    for (var i = 0u; i < words_to_copy; i++) {
        transfer_destination[destination_word_offset + i] = transfer_source.data[data_word_offset + i];
    }
}