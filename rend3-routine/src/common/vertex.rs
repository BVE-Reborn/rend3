use rend3::managers::{
    VERTEX_COLOR_SLOT, VERTEX_JOINT_INDEX_SLOT, VERTEX_JOINT_WEIGHT_SLOT, VERTEX_NORMAL_SLOT, VERTEX_OBJECT_INDEX_SLOT,
    VERTEX_POSITION_SLOT, VERTEX_TANGENT_SLOT, VERTEX_UV0_SLOT, VERTEX_UV1_SLOT,
};
use wgpu::{VertexAttribute, VertexBufferLayout, VertexFormat, VertexStepMode};

/// Vertex buffer layouts used when CpuDriven.
pub static CPU_VERTEX_BUFFERS: [VertexBufferLayout<'static>; 8] = [
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_POSITION_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 0,
            shader_location: VERTEX_POSITION_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_NORMAL_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 0,
            shader_location: VERTEX_NORMAL_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_TANGENT_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 0,
            shader_location: VERTEX_TANGENT_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_UV_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: VERTEX_UV0_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_UV_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: VERTEX_UV1_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_COLOR_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Unorm8x4,
            offset: 0,
            shader_location: VERTEX_COLOR_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_JOINT_INDEX_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Uint16x4,
            offset: 0,
            shader_location: VERTEX_JOINT_INDEX_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_JOINT_WEIGHT_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x4,
            offset: 0,
            shader_location: VERTEX_JOINT_WEIGHT_SLOT,
        }],
    },
];

/// Vertex buffer layouts used when GpuDriven.
pub static GPU_VERTEX_BUFFERS: [VertexBufferLayout<'static>; 9] = [
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_POSITION_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 0,
            shader_location: VERTEX_POSITION_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_NORMAL_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 0,
            shader_location: VERTEX_NORMAL_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_TANGENT_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x3,
            offset: 0,
            shader_location: VERTEX_TANGENT_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_UV_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: VERTEX_UV0_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_UV_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Float32x2,
            offset: 0,
            shader_location: VERTEX_UV1_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_COLOR_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Unorm8x4,
            offset: 0,
            shader_location: VERTEX_COLOR_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_JOINT_INDEX_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Uint16x4,
            offset: 0,
            shader_location: VERTEX_JOINT_INDEX_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: rend3::managers::VERTEX_JOINT_WEIGHT_SIZE as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[VertexAttribute {
            format: VertexFormat::Uint16x4,
            offset: 0,
            shader_location: VERTEX_JOINT_WEIGHT_SLOT,
        }],
    },
    VertexBufferLayout {
        array_stride: 20,
        step_mode: VertexStepMode::Instance,
        attributes: &[VertexAttribute {
            format: VertexFormat::Uint32,
            offset: 16,
            shader_location: VERTEX_OBJECT_INDEX_SLOT,
        }],
    },
];
