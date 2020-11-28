use crate::{
    list::{ShaderSourceStage, ShaderSourceType, SourceShaderDescriptor},
    renderer::shaders::ShaderManager,
};
use std::{future::Future, mem::size_of, num::NonZeroU64, ops::Range};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, BufferSlice, ComputePass, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, ProgrammableStageDescriptor, PushConstantRange, ShaderStage,
};

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct ShaderCopyInput {
    start_in: u32,
    count: u32,
    start_out: u32,
}

unsafe impl bytemuck::Zeroable for ShaderCopyInput {}
unsafe impl bytemuck::Pod for ShaderCopyInput {}

#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct ShaderCopyOffsetInput {
    start_in: u32,
    count: u32,
    start_out: u32,
    offset: i32,
}

unsafe impl bytemuck::Zeroable for ShaderCopyOffsetInput {}
unsafe impl bytemuck::Pod for ShaderCopyOffsetInput {}

pub struct GpuCopyData {
    bind_group: BindGroup,
}

pub struct GpuCopy {
    layout: BindGroupLayout,
    copy_pipeline: ComputePipeline,
    copy_offset_pipeline: ComputePipeline,
    subgroup_size: u32,
}
impl GpuCopy {
    pub fn new<'a>(
        device: &'a Device,
        shader_manager: &ShaderManager,
        subgroup_size: u32,
    ) -> impl Future<Output = Self> + 'a {
        let copy_shader = shader_manager.compile_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin(String::from("copy.comp")),
            defines: vec![(String::from("WARP_SIZE"), Some(subgroup_size.to_string()))],
            includes: vec![],
            stage: ShaderSourceStage::Compute,
        });

        let copy_offset_shader = shader_manager.compile_shader(SourceShaderDescriptor {
            source: ShaderSourceType::Builtin(String::from("copy_offset.comp")),
            defines: vec![(String::from("WARP_SIZE"), Some(subgroup_size.to_string()))],
            includes: vec![],
            stage: ShaderSourceStage::Compute,
        });

        let layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("copy bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStage::COMPUTE,
                    ty: BindingType::StorageBuffer {
                        readonly: true,
                        dynamic: false,
                        min_binding_size: NonZeroU64::new(4),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStage::COMPUTE,
                    ty: BindingType::StorageBuffer {
                        readonly: false,
                        dynamic: false,
                        min_binding_size: NonZeroU64::new(4),
                    },
                    count: None,
                },
            ],
        });

        let copy_pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("copy pll"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[PushConstantRange {
                stages: ShaderStage::COMPUTE,
                range: 0..size_of::<ShaderCopyInput>() as _,
            }],
        });

        let copy_offset_pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("copy offset pll"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[PushConstantRange {
                stages: ShaderStage::COMPUTE,
                range: 0..size_of::<ShaderCopyOffsetInput>() as _,
            }],
        });

        async move {
            let copy_shader = copy_shader.await.unwrap();
            let copy_offset_shader = copy_offset_shader.await.unwrap();

            let copy_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("copy pipeline"),
                layout: Some(&copy_pll),
                compute_stage: ProgrammableStageDescriptor {
                    module: &copy_shader,
                    entry_point: "main",
                },
            });

            let copy_offset_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("copy offset pipeline"),
                layout: Some(&copy_offset_pll),
                compute_stage: ProgrammableStageDescriptor {
                    module: &copy_offset_shader,
                    entry_point: "main",
                },
            });

            Self {
                layout,
                copy_pipeline,
                copy_offset_pipeline,
                subgroup_size,
            }
        }
    }

    pub fn prepare(
        &self,
        device: &Device,
        input: BufferSlice<'_>,
        output: BufferSlice<'_>,
        label: &str,
    ) -> GpuCopyData {
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some(label),
            layout: &self.layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Buffer(input),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Buffer(output),
                },
            ],
        });

        GpuCopyData { bind_group }
    }

    /// Input range and offset are in 4 byte words
    pub fn copy_words<'a>(
        &'a self,
        cpass: &mut ComputePass<'a>,
        data: &'a GpuCopyData,
        input: Range<u32>,
        output_offset: u32,
    ) {
        let dispatches = ((input.len() + self.subgroup_size as usize - 1) / self.subgroup_size as usize) as u32;

        let input = ShaderCopyInput {
            start_in: input.start,
            count: input.len() as _,
            start_out: output_offset,
        };

        cpass.set_pipeline(&self.copy_pipeline);
        cpass.set_push_constants(0, bytemuck::cast_slice(&[input]));
        cpass.set_bind_group(0, &data.bind_group, &[]);
        cpass.dispatch(dispatches, 1, 1);
    }

    /// Input range and offsets are in 4 byte words
    pub fn copy_words_with_offset<'a>(
        &'a self,
        cpass: &mut ComputePass<'a>,
        data: &'a GpuCopyData,
        input: Range<u32>,
        output_offset: u32,
        offset: i32,
    ) {
        let dispatches = ((input.len() + self.subgroup_size as usize - 1) / self.subgroup_size as usize) as u32;

        let input = ShaderCopyOffsetInput {
            start_in: input.start,
            count: input.len() as _,
            start_out: output_offset,
            offset,
        };

        cpass.set_pipeline(&self.copy_offset_pipeline);
        cpass.set_push_constants(0, bytemuck::cast_slice(&[input]));
        cpass.set_bind_group(0, &data.bind_group, &[]);
        cpass.dispatch(dispatches, 1, 1);
    }
}
