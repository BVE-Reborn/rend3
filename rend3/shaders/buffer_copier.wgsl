// This compute shader is used to copy one portion of the allocated vertex
// megabuffer to another non-overlapping region of the same buffer.

// The arrays are tightly packed, so we can't use types like vec3 which don't
// have the right alignment. Instead, we manually create these structs to ensure
// arrays will get the right stride.
struct Vec4 { x: u32; y: u32; z: u32; w: u32; };
struct Vec3 { x: u32; y: u32; z: u32; };
struct Vec2 { x: u32; y: u32; };
struct Color { rgba: u32; };

struct Vec4Array { d: [[stride(16)]] array<Vec4>; };

struct Vec3Array { d: [[stride(12)]] array<Vec3>; };

struct Vec2Array { d: [[stride(8)]] array<Vec2>; };

struct ColorArray { d: [[stride(4)]] array<Color>; };

struct BufferCopierParams {
    src_offset: u32;
    dst_offset: u32;
    count: u32;
};

[[group(0), binding(0)]]
var<storage, read_write> positions: Vec3Array;

[[group(0), binding(1)]]
var<storage, read_write> normals: Vec3Array;

[[group(0), binding(2)]]
var<storage, read_write> tangents: Vec3Array;

[[group(0), binding(3)]]
var<storage, read_write> uv0s: Vec2Array;

[[group(0), binding(4)]]
var<storage, read_write> uv1s: Vec2Array;

[[group(0), binding(5)]]
var<storage, read_write> colors: ColorArray;

[[group(0), binding(6)]]
/// It's a vec4<u16>, so we can treat it as a vec2<u32> for the copy
var<storage, read_write> joint_indices: Vec2Array;

[[group(0), binding(7)]]
var<storage, read_write> joint_weights: Vec4Array;

[[group(0), binding(8)]]
var<uniform> params: BufferCopierParams;

[[stage(compute), workgroup_size(256)]]
fn main([[builtin(global_invocation_id)]] global_id: vec3<u32>) {

    let i = global_id.x;
    if (i >= params.count) {
        return;
    }

    positions.d     [i + params.dst_offset] = positions.d     [i + params.src_offset];
    normals.d       [i + params.dst_offset] = normals.d       [i + params.src_offset];
    tangents.d      [i + params.dst_offset] = tangents.d      [i + params.src_offset];
    uv0s.d          [i + params.dst_offset] = uv0s.d          [i + params.src_offset];
    uv1s.d          [i + params.dst_offset] = uv1s.d          [i + params.src_offset];
    colors.d        [i + params.dst_offset] = colors.d        [i + params.src_offset];
    joint_indices.d [i + params.dst_offset] = joint_indices.d [i + params.src_offset];
    joint_weights.d [i + params.dst_offset] = joint_weights.d [i + params.src_offset];
}