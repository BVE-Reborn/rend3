//! Compute shader copier for vertex buffers.

use wgpu::{
    util::DeviceExt, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutEntry, CommandEncoder,
    ComputePipeline, Device,
};

use crate::util::math::round_up_div;

/// Parameters for a buffer copy.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug)]
pub struct VertexBufferCopierParams {
    /// Offset, in vertices, to start copying from.
    pub src_offset: u32,
    /// Offset, in vertices, to start copying to. dst_offset + count must not
    /// intersect src_offset + count
    pub dst_offset: u32,
    /// Size of the copy
    pub count: u32,
}

// SAFETY: The type only contains u32 values.
unsafe impl bytemuck::Pod for VertexBufferCopierParams {}
unsafe impl bytemuck::Zeroable for VertexBufferCopierParams {}

/// Copies vertex buffers from one location to another via compute shader.
///
/// When allocating data for a new skeleton, the MeshManager needs to copy some
/// of the allocated vertex data to a different region of the vertex buffer.
/// This copy operation can't be done by wgpu using the `Encoder` because it is
/// not supported by the spec to copy within the same buffer. Instead, this
/// struct is responsible for creating and executing a compute pass that
/// performs the same operation.
pub struct VertexBufferCopier {
    pub pipeline: ComputePipeline,
    pub bgl: BindGroupLayout,
}

impl VertexBufferCopier {
    const WORKGROUP_SIZE: u32 = 256;

    pub fn new(device: &Device) -> Self {
        let buffer_entry = |idx| BindGroupLayoutEntry {
            binding: idx,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: false },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("BufferCopier"),
            entries: &[
                buffer_entry(0), // Position
                buffer_entry(1), // Normal
                buffer_entry(2), // Tangent
                buffer_entry(3), // UV0
                buffer_entry(4), // UV1
                buffer_entry(5), // Color
                buffer_entry(6), // Joint index
                buffer_entry(7), // Joint weight
                BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("BufferCopier Pipeline Layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let module = device.create_shader_module(&wgpu::ShaderModuleDescriptor {
            label: Some("BufferCopier Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/buffer_copier.wgsl").into()),
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            layout: Some(&pipeline_layout),
            label: Some("BufferCopier Pipeline"),
            module: &module,
            entry_point: "main",
        });

        VertexBufferCopier { pipeline, bgl }
    }

    pub fn execute(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        buffers: [&wgpu::Buffer; 8],
        params: VertexBufferCopierParams,
    ) {
        let buffer_binding = |idx, buffer| BindGroupEntry {
            binding: idx,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: None,
            }),
        };

        let params_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&[params]),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("BufferCopier Bind Group"),
            layout: &self.bgl,
            entries: &[
                buffer_binding(0, buffers[0]),
                buffer_binding(1, buffers[1]),
                buffer_binding(2, buffers[2]),
                buffer_binding(3, buffers[3]),
                buffer_binding(4, buffers[4]),
                buffer_binding(5, buffers[5]),
                buffer_binding(6, buffers[6]),
                buffer_binding(7, buffers[7]),
                BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &params_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
            ],
        });

        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("BufferCopier Compute Pass"),
        });

        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, &bind_group, &[]);
        let num_workgroups = round_up_div(params.count, Self::WORKGROUP_SIZE);
        cpass.dispatch(num_workgroups, 1, 1);

        drop(cpass);
    }
}
