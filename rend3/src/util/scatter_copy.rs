use std::{num::NonZeroU64};

use encase::{private::WriteInto, ShaderSize};
use wgpu::{
    BindGroupLayout, BindingType, Buffer, BufferBindingType, BufferDescriptor, BufferUsages, CommandEncoder,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, ShaderStages,
};

use crate::util::{
    bind_merge::{BindGroupBuilder, BindGroupLayoutBuilder},
    math::round_up_div,
};

pub struct ScatterData<T> {
    word_offset: u32,
    data: T,
}
impl<T> ScatterData<T> {
    pub fn new(byte_offset: u32, data: T) -> Self {
        Self {
            word_offset: byte_offset / 4,
            data,
        }
    }
}

pub struct ScatterCopy {
    pipeline: ComputePipeline,
    bgl: BindGroupLayout,
}
impl ScatterCopy {
    pub fn new(device: &Device) -> Self {
        let sm = device.create_shader_module(wgpu::include_wgsl!("../../shaders/scatter_copy.wgsl"));

        let bgl = BindGroupLayoutBuilder::new()
            .append(
                ShaderStages::COMPUTE,
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(12).unwrap()),
                },
                None,
            )
            .append(
                ShaderStages::COMPUTE,
                BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                },
                None,
            )
            .build(&device, Some("ScatterCopy bgl"));

        let pll = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("ScatterCopy pll"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ScatterCopy compute pipeline"),
            layout: Some(&pll),
            module: &sm,
            entry_point: "cs_main",
        });

        Self { pipeline, bgl }
    }

    pub fn execute_copy<T, D>(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        destination_buffer: &Buffer,
        data: D,
    ) where
        T: ShaderSize + WriteInto,
        D: IntoIterator<Item = ScatterData<T>>,
        D::IntoIter: ExactSizeIterator,
    {
        let data_iterator = data.into_iter();

        let size_of_t = T::SHADER_SIZE.get();
        assert_eq!(size_of_t % 4, 0);
        let size_of_t_u32: u32 = size_of_t.try_into().unwrap();

        let count = data_iterator.len() as u64;

        let stride_bytes = size_of_t + 4;
        let stride_words = (stride_bytes / 4) as usize;

        let buffer_size = count * stride_bytes + 8;
        let source_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("ScatterCopy temporary source buffer"),
            size: buffer_size,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: true,
        });
        let mut mapped_range = source_buffer.slice(..).get_mapped_range_mut();
        let mapped_slice: &mut [u32] = bytemuck::cast_slice_mut(&mut mapped_range);

        let count_u32: u32 = count.try_into().unwrap();
        mapped_slice[0] = size_of_t_u32;
        mapped_slice[1] = count_u32;

        for (idx, item) in data_iterator.enumerate() {
            // Add two words for the header.
            let range_start = idx * stride_words + 2;
            let range_end = range_start + stride_words;

            mapped_slice[range_start] = item.word_offset;
            let mut writer = encase::internal::Writer::new(
                &item.data,
                bytemuck::cast_slice_mut(&mut mapped_slice[range_start + 1..range_end]),
                0,
            )
            .unwrap();
            item.data.write_into(&mut writer);
        }

        source_buffer.unmap();

        let bg = BindGroupBuilder::new()
            .append_buffer(&source_buffer)
            .append_buffer(destination_buffer)
            .build(device, Some("ScatterCopy temporary bind group"), &self.bgl);

        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("ScatterCopy cpass"),
        });
        cpass.set_pipeline(&self.pipeline);
        cpass.set_bind_group(0, &bg, &[]);
        cpass.dispatch_workgroups(round_up_div(count_u32, 64), 1, 1);
        drop(cpass);
    }
}
