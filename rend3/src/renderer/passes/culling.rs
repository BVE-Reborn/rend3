use crate::{
    list::{ShaderSourceStage, ShaderSourceType, SourceShaderDescriptor},
    renderer::shaders::ShaderManager,
};
use std::future::Future;
use tracing_futures::Instrument;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, Buffer, BufferAddress, BufferDescriptor,
    BufferUsage, ComputePass, ComputePipeline, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor,
    ProgrammableStageDescriptor, PushConstantRange, ShaderStage,
};

const SIZE_OF_STATUS: BufferAddress = 4;
const SIZE_OF_INDEX: BufferAddress = 4;
const SIZE_OF_OUTPUT_DATA: BufferAddress = 12 * 16;
const SIZE_OF_INDIRECT_CALL: BufferAddress = 5 * 4;
const SIZE_OF_INDIRECT_COUNT: BufferAddress = 4;

pub struct CullingPassData {
    pub name: String,
    pub pre_cull_bg: BindGroup,
    pub prefix_sum_bg1: BindGroup,
    pub prefix_sum_bg2: BindGroup,
    pub output_bg: BindGroup,
    pub output_buffer: Buffer,
    pub indirect_buffer: Buffer,
    pub count_buffer: Buffer,
    pub object_count: u32,
}

pub struct CullingPass {
    pre_cull_pipeline: ComputePipeline,
    prefix_sum_pipeline: ComputePipeline,
    post_cull_pipeline: ComputePipeline,
    subgroup_size: u32,
}
impl CullingPass {
    pub fn new<'a>(
        device: &'a Device,
        shader_manager: &ShaderManager,
        prefix_sum_bgl: &BindGroupLayout,
        pre_cull_bgl: &BindGroupLayout,
        object_input_bgl: &BindGroupLayout,
        output_bgl: &BindGroupLayout,
        uniform_bgl: &BindGroupLayout,
        subgroup_size: u32,
    ) -> impl Future<Output = Self> + 'a {
        let new_span = tracing::warn_span!("Creating CullingPass");
        let new_span_guard = new_span.enter();

        let pre_cull_shader = shader_manager.compile_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File(String::from("rend3/shaders/pre_cull.comp")),
            defines: vec![(String::from("WARP_SIZE"), Some(subgroup_size.to_string()))],
            includes: vec![],
            stage: ShaderSourceStage::Compute,
        });

        let prefix_sum = shader_manager.compile_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File(String::from("rend3/shaders/prefix_sum.comp")),
            defines: vec![(String::from("WARP_SIZE"), Some(subgroup_size.to_string()))],
            includes: vec![],
            stage: ShaderSourceStage::Compute,
        });

        let post_cull_shader = shader_manager.compile_shader(SourceShaderDescriptor {
            source: ShaderSourceType::File(String::from("rend3/shaders/post_cull.comp")),
            defines: vec![(String::from("WARP_SIZE"), Some(subgroup_size.to_string()))],
            includes: vec![],
            stage: ShaderSourceStage::Compute,
        });

        let pre_cull_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("pre-cull pipeline layout"),
            bind_group_layouts: &[object_input_bgl, pre_cull_bgl, uniform_bgl],
            push_constant_ranges: &[PushConstantRange {
                range: 0..4,
                stages: ShaderStage::COMPUTE,
            }],
        });

        let prefix_sum_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("prefix-sum pipeline layout"),
            bind_group_layouts: &[prefix_sum_bgl],
            push_constant_ranges: &[PushConstantRange {
                range: 0..8,
                stages: ShaderStage::COMPUTE,
            }],
        });

        let post_cull_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("post-cull pipeline layout"),
            bind_group_layouts: &[object_input_bgl, output_bgl, uniform_bgl],
            push_constant_ranges: &[PushConstantRange {
                range: 0..4,
                stages: ShaderStage::COMPUTE,
            }],
        });

        drop(new_span_guard);

        async move {
            let pre_cull_shader = pre_cull_shader.await.unwrap();
            let prefix_sum = prefix_sum.await.unwrap();
            let post_cull_shader = post_cull_shader.await.unwrap();

            let pre_cull_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("culling pipeline"),
                layout: Some(&pre_cull_pipeline_layout),
                compute_stage: ProgrammableStageDescriptor {
                    module: &pre_cull_shader,
                    entry_point: "main",
                },
            });

            let prefix_sum_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("prefix-sum pipeline"),
                layout: Some(&prefix_sum_pipeline_layout),
                compute_stage: ProgrammableStageDescriptor {
                    module: &prefix_sum,
                    entry_point: "main",
                },
            });

            let post_cull_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
                label: Some("post-cull pipeline"),
                layout: Some(&post_cull_pipeline_layout),
                compute_stage: ProgrammableStageDescriptor {
                    module: &post_cull_shader,
                    entry_point: "main",
                },
            });

            Self {
                pre_cull_pipeline,
                prefix_sum_pipeline,
                post_cull_pipeline,
                subgroup_size,
            }
        }
        .instrument(new_span)
    }

    pub fn prepare<'a>(
        &'a self,
        device: &Device,
        prefix_sum_bgl: &BindGroupLayout,
        pre_cull_bgl: &BindGroupLayout,
        output_bgl: &BindGroupLayout,
        object_count: u32,
        name: String,
    ) -> CullingPassData {
        span_transfer!(_ -> prepare_span, WARN, "Preparing CullingPass");

        let status_buffer = device.create_buffer(&BufferDescriptor {
            label: Some(&*format!("status buffer for {}", &name)),
            size: SIZE_OF_STATUS * object_count as BufferAddress,
            usage: BufferUsage::STORAGE,
            mapped_at_creation: false,
        });

        let index_buffer1 = device.create_buffer(&BufferDescriptor {
            label: Some(&*format!("index buffer 1 for {}", &name)),
            size: SIZE_OF_INDEX * object_count as BufferAddress,
            usage: BufferUsage::STORAGE,
            mapped_at_creation: false,
        });

        let index_buffer2 = device.create_buffer(&BufferDescriptor {
            label: Some(&*format!("index buffer 2 for {}", &name)),
            size: SIZE_OF_INDEX * object_count as BufferAddress,
            usage: BufferUsage::STORAGE,
            mapped_at_creation: false,
        });

        let output_buffer = device.create_buffer(&BufferDescriptor {
            label: Some(&*format!("object output buffer for {}", &name)),
            size: SIZE_OF_OUTPUT_DATA * object_count as BufferAddress,
            usage: BufferUsage::STORAGE,
            mapped_at_creation: false,
        });

        let indirect_buffer = device.create_buffer(&BufferDescriptor {
            label: Some(&*format!("indirect buffer for {}", &name)),
            size: SIZE_OF_INDIRECT_CALL * object_count as BufferAddress,
            usage: BufferUsage::STORAGE | BufferUsage::INDIRECT | BufferUsage::VERTEX,
            mapped_at_creation: false,
        });

        let count_buffer = device.create_buffer(&BufferDescriptor {
            label: Some(&*format!("count buffer for {}", &name)),
            size: SIZE_OF_INDIRECT_COUNT,
            usage: BufferUsage::STORAGE | BufferUsage::INDIRECT,
            mapped_at_creation: true,
        });

        count_buffer
            .slice(..)
            .get_mapped_range_mut()
            .copy_from_slice(bytemuck::bytes_of(&0));
        count_buffer.unmap();

        let count = (object_count as f32).log2().ceil() as u32;

        let pre_cull_bg = device.create_bind_group(&BindGroupDescriptor {
            label: Some(&*format!("pre-cull bind group for {}", &name)),
            layout: pre_cull_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: index_buffer1.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: status_buffer.as_entire_binding(),
                },
            ],
        });

        let prefix_sum_bg1 = device.create_bind_group(&BindGroupDescriptor {
            label: Some(&*format!("prefix-sum bind group 1 for {}", &name)),
            layout: &prefix_sum_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: index_buffer1.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: index_buffer2.as_entire_binding(),
                },
            ],
        });

        let prefix_sum_bg2 = device.create_bind_group(&BindGroupDescriptor {
            label: Some(&*format!("prefix-sum bind group 2 for {}", &name)),
            layout: &prefix_sum_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: index_buffer2.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: index_buffer1.as_entire_binding(),
                },
            ],
        });

        let index_buffer = if count % 2 == 0 { &index_buffer1 } else { &index_buffer2 };

        let output_bg = device.create_bind_group(&BindGroupDescriptor {
            label: Some(&*format!("output bind group for {}", &name)),
            layout: output_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: index_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: status_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: output_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: indirect_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: count_buffer.as_entire_binding(),
                },
            ],
        });

        CullingPassData {
            name,
            pre_cull_bg,
            prefix_sum_bg1,
            prefix_sum_bg2,
            output_bg,
            output_buffer,
            indirect_buffer,
            count_buffer,
            object_count,
        }
    }

    pub fn run<'a>(
        &'a self,
        cpass: &mut ComputePass<'a>,
        object_input_bg: &'a BindGroup,
        uniform_bg: &'a BindGroup,
        data: &'a CullingPassData,
    ) {
        let dispatch_count = (data.object_count + self.subgroup_size - 1) / self.subgroup_size;

        span_transfer!(_ -> run_span, WARN, "Running CullingPass");
        cpass.set_pipeline(&self.pre_cull_pipeline);
        cpass.set_push_constants(0, bytemuck::bytes_of(&data.object_count));
        cpass.set_bind_group(0, object_input_bg, &[]);
        cpass.set_bind_group(1, &data.pre_cull_bg, &[]);
        cpass.set_bind_group(2, uniform_bg, &[]);
        cpass.dispatch(dispatch_count, 1, 1);

        cpass.set_pipeline(&self.prefix_sum_pipeline);
        let mut stride = 1_u32;
        let mut iteration = 0;
        while stride < data.object_count {
            cpass.set_push_constants(0, bytemuck::cast_slice(&[stride, data.object_count]));
            let bind_group = if iteration % 2 == 0 {
                &data.prefix_sum_bg1
            } else {
                &data.prefix_sum_bg2
            };
            cpass.set_bind_group(0, bind_group, &[]);
            cpass.dispatch(dispatch_count, 1, 1);
            stride <<= 1;
            iteration += 1;
        }

        cpass.set_pipeline(&self.post_cull_pipeline);
        cpass.set_push_constants(0, bytemuck::bytes_of(&data.object_count));
        cpass.set_bind_group(0, object_input_bg, &[]);
        cpass.set_bind_group(1, &data.output_bg, &[]);
        cpass.set_bind_group(2, uniform_bg, &[]);
        cpass.dispatch(dispatch_count, 1, 1);
    }
}
