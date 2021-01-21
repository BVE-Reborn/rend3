pub use crate::renderer::culling::cpu::CPUDrawCall;
use crate::{
    list::{ShaderSourceStage, ShaderSourceType, SourceShaderDescriptor},
    mode::ModeData,
    renderer::{camera::CameraManager, object::ObjectManager, shaders::ShaderManager},
    JobPriorities, RendererMode,
};
use futures::future::Either;
use std::future::Future;
use switchyard::Switchyard;
use tracing_futures::Instrument;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Buffer, BufferAddress,
    BufferDescriptor, BufferUsage, ComputePass, ComputePipeline, ComputePipelineDescriptor, Device,
    PipelineLayoutDescriptor, ProgrammableStageDescriptor, PushConstantRange, Queue, ShaderStage,
};

mod cpu;

const SIZE_OF_STATUS: BufferAddress = 4;
const SIZE_OF_INDEX: BufferAddress = 4;
const SIZE_OF_OUTPUT_DATA: BufferAddress = 12 * 16;
const SIZE_OF_INDIRECT_CALL: BufferAddress = 5 * 4;
const SIZE_OF_INDIRECT_COUNT: BufferAddress = 4;

pub(crate) struct GPUCullingPassData {
    pub pre_cull_bg: BindGroup,
    pub prefix_sum_bg1: BindGroup,
    pub prefix_sum_bg2: BindGroup,
    pub output_bg: BindGroup,
    pub indirect_buffer: Buffer,
    pub count_buffer: Buffer,
}

pub(crate) struct CullingPassData {
    pub name: String,
    pub inner: ModeData<Vec<CPUDrawCall>, GPUCullingPassData>,
    pub output_buffer: Buffer,
    pub object_count: u32,
}

pub struct GPUCullingPass {
    pre_cull_pipeline: ComputePipeline,
    prefix_sum_pipeline: ComputePipeline,
    post_cull_pipeline: ComputePipeline,
    subgroup_size: u32,
}

pub struct CullingPassCreationArgs<'a> {
    pub mode: RendererMode,
    pub shader_manager: &'a ShaderManager,
    pub prefix_sum_bgl: &'a BindGroupLayout,
    pub pre_cull_bgl: &'a BindGroupLayout,
    pub object_input_bgl: &'a BindGroupLayout,
    pub output_bgl: &'a BindGroupLayout,
    pub uniform_bgl: &'a BindGroupLayout,
    pub subgroup_size: u32,
}

pub struct CullingPassPrepareArgs<'a> {
    pub device: &'a Device,
    pub mode: RendererMode,
    pub prefix_sum_bgl: &'a BindGroupLayout,
    pub pre_cull_bgl: &'a BindGroupLayout,
    pub output_bgl: &'a BindGroupLayout,
    pub object_count: u32,
    pub name: String,
}

pub struct CullingPass {
    inner: ModeData<(), GPUCullingPass>,
}
impl CullingPass {
    pub fn new<'a, 'b>(device: &'a Device, args: CullingPassCreationArgs<'b>) -> impl Future<Output = Self> + 'a {
        let new_span = tracing::warn_span!("Creating CullingPass");
        let new_span_guard = new_span.enter();

        if args.mode == RendererMode::GPUPowered {
            let pre_cull_shader = args.shader_manager.compile_shader(SourceShaderDescriptor {
                source: ShaderSourceType::Builtin(String::from("pre_cull.comp")),
                defines: vec![(String::from("WARP_SIZE"), Some(args.subgroup_size.to_string()))],
                includes: vec![],
                stage: ShaderSourceStage::Compute,
            });

            let prefix_sum = args.shader_manager.compile_shader(SourceShaderDescriptor {
                source: ShaderSourceType::Builtin(String::from("prefix_sum.comp")),
                defines: vec![(String::from("WARP_SIZE"), Some(args.subgroup_size.to_string()))],
                includes: vec![],
                stage: ShaderSourceStage::Compute,
            });

            let post_cull_shader = args.shader_manager.compile_shader(SourceShaderDescriptor {
                source: ShaderSourceType::Builtin(String::from("post_cull.comp")),
                defines: vec![(String::from("WARP_SIZE"), Some(args.subgroup_size.to_string()))],
                includes: vec![],
                stage: ShaderSourceStage::Compute,
            });

            let pre_cull_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("pre-cull pipeline layout"),
                bind_group_layouts: &[args.object_input_bgl, args.pre_cull_bgl, args.uniform_bgl],
                push_constant_ranges: &[PushConstantRange {
                    range: 0..4,
                    stages: ShaderStage::COMPUTE,
                }],
            });

            let prefix_sum_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("prefix-sum pipeline layout"),
                bind_group_layouts: &[args.prefix_sum_bgl],
                push_constant_ranges: &[PushConstantRange {
                    range: 0..8,
                    stages: ShaderStage::COMPUTE,
                }],
            });

            let post_cull_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("post-cull pipeline layout"),
                bind_group_layouts: &[args.object_input_bgl, args.output_bgl, args.uniform_bgl],
                push_constant_ranges: &[PushConstantRange {
                    range: 0..4,
                    stages: ShaderStage::COMPUTE,
                }],
            });

            drop(new_span_guard);

            // Need to not keep arguments alive
            let subgroup_size = args.subgroup_size;

            Either::Left(
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
                        inner: ModeData::GPU(GPUCullingPass {
                            pre_cull_pipeline,
                            prefix_sum_pipeline,
                            post_cull_pipeline,
                            subgroup_size,
                        }),
                    }
                }
                .instrument(new_span),
            )
        } else {
            Either::Right(async {
                Self {
                    inner: ModeData::CPU(()),
                }
            })
        }
    }

    pub(crate) fn prepare(&self, args: CullingPassPrepareArgs<'_>) -> CullingPassData {
        span_transfer!(_ -> prepare_span, WARN, "Preparing CullingPass");

        let output_buffer = args.device.create_buffer(&BufferDescriptor {
            label: Some(&*format!("object output buffer for {}", &args.name)),
            size: SIZE_OF_OUTPUT_DATA * args.object_count as BufferAddress,
            usage: match args.mode {
                RendererMode::CPUPowered => BufferUsage::COPY_DST | BufferUsage::STORAGE,
                RendererMode::GPUPowered => BufferUsage::STORAGE,
            },
            mapped_at_creation: false,
        });

        let inner = args.mode.into_data(Vec::new, || {
            let status_buffer = args.device.create_buffer(&BufferDescriptor {
                label: Some(&*format!("status buffer for {}", &args.name)),
                size: SIZE_OF_STATUS * args.object_count as BufferAddress,
                usage: BufferUsage::STORAGE,
                mapped_at_creation: false,
            });

            let index_buffer1 = args.device.create_buffer(&BufferDescriptor {
                label: Some(&*format!("index buffer 1 for {}", &args.name)),
                size: SIZE_OF_INDEX * args.object_count as BufferAddress,
                usage: BufferUsage::STORAGE,
                mapped_at_creation: false,
            });

            let index_buffer2 = args.device.create_buffer(&BufferDescriptor {
                label: Some(&*format!("index buffer 2 for {}", &args.name)),
                size: SIZE_OF_INDEX * args.object_count as BufferAddress,
                usage: BufferUsage::STORAGE,
                mapped_at_creation: false,
            });

            let indirect_buffer = args.device.create_buffer(&BufferDescriptor {
                label: Some(&*format!("indirect buffer for {}", &args.name)),
                size: SIZE_OF_INDIRECT_CALL * args.object_count as BufferAddress,
                usage: BufferUsage::STORAGE | BufferUsage::INDIRECT | BufferUsage::VERTEX,
                mapped_at_creation: false,
            });

            let count_buffer = args.device.create_buffer(&BufferDescriptor {
                label: Some(&*format!("count buffer for {}", &args.name)),
                size: SIZE_OF_INDIRECT_COUNT,
                usage: BufferUsage::STORAGE | BufferUsage::INDIRECT,
                mapped_at_creation: true,
            });

            count_buffer
                .slice(..)
                .get_mapped_range_mut()
                .copy_from_slice(bytemuck::bytes_of(&0));
            count_buffer.unmap();

            let count = (args.object_count as f32).log2().ceil() as u32;

            let pre_cull_bg = args.device.create_bind_group(&BindGroupDescriptor {
                label: Some(&*format!("pre-cull bind group for {}", &args.name)),
                layout: args.pre_cull_bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::Buffer(index_buffer1.slice(..)),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Buffer(status_buffer.slice(..)),
                    },
                ],
            });

            let prefix_sum_bg1 = args.device.create_bind_group(&BindGroupDescriptor {
                label: Some(&*format!("prefix-sum bind group 1 for {}", &args.name)),
                layout: &args.prefix_sum_bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::Buffer(index_buffer1.slice(..)),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Buffer(index_buffer2.slice(..)),
                    },
                ],
            });

            let prefix_sum_bg2 = args.device.create_bind_group(&BindGroupDescriptor {
                label: Some(&*format!("prefix-sum bind group 2 for {}", &args.name)),
                layout: &args.prefix_sum_bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::Buffer(index_buffer2.slice(..)),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Buffer(index_buffer1.slice(..)),
                    },
                ],
            });

            let index_buffer = if count % 2 == 0 { &index_buffer1 } else { &index_buffer2 };

            let output_bg = args.device.create_bind_group(&BindGroupDescriptor {
                label: Some(&*format!("output bind group for {}", &args.name)),
                layout: args.output_bgl,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: BindingResource::Buffer(index_buffer.slice(..)),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::Buffer(status_buffer.slice(..)),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::Buffer(output_buffer.slice(..)),
                    },
                    BindGroupEntry {
                        binding: 3,
                        resource: BindingResource::Buffer(indirect_buffer.slice(..)),
                    },
                    BindGroupEntry {
                        binding: 4,
                        resource: BindingResource::Buffer(count_buffer.slice(..)),
                    },
                ],
            });

            GPUCullingPassData {
                pre_cull_bg,
                prefix_sum_bg1,
                prefix_sum_bg2,
                output_bg,
                indirect_buffer,
                count_buffer,
            }
        });

        CullingPassData {
            name: args.name,
            inner,
            output_buffer,
            object_count: args.object_count,
        }
    }

    pub(crate) fn cpu_run<'a, TD>(
        &self,
        yard: &'a Switchyard<TD>,
        yard_priorities: JobPriorities,
        queue: &'a Queue,
        object_manager: &'a ObjectManager,
        data: &'a mut CullingPassData,
        camera: CameraManager,
    ) -> impl Future<Output = ()> + 'a
    where
        TD: 'static,
    {
        cpu::run(yard, yard_priorities, queue, object_manager, data, camera)
    }

    pub(crate) fn gpu_run<'a>(
        &'a self,
        cpass: &mut ComputePass<'a>,
        object_input_bg: &'a BindGroup,
        uniform_bg: &'a BindGroup,
        data: &'a CullingPassData,
    ) {
        let cull_pass = self.inner.as_gpu();
        let dispatch_count = (data.object_count + cull_pass.subgroup_size - 1) / cull_pass.subgroup_size;

        span_transfer!(_ -> run_span, WARN, "Running CullingPass");
        cpass.set_pipeline(&cull_pass.pre_cull_pipeline);
        cpass.set_push_constants(0, &[data.object_count]);
        cpass.set_bind_group(0, object_input_bg, &[]);
        cpass.set_bind_group(1, &data.inner.as_gpu().pre_cull_bg, &[]);
        cpass.set_bind_group(2, uniform_bg, &[]);
        cpass.dispatch(dispatch_count, 1, 1);

        cpass.set_pipeline(&cull_pass.prefix_sum_pipeline);
        let mut stride = 1_u32;
        let mut iteration = 0;
        while stride < data.object_count {
            cpass.set_push_constants(0, bytemuck::cast_slice(&[stride, data.object_count]));
            let bind_group = if iteration % 2 == 0 {
                &data.inner.as_gpu().prefix_sum_bg1
            } else {
                &data.inner.as_gpu().prefix_sum_bg2
            };
            cpass.set_bind_group(0, bind_group, &[]);
            cpass.dispatch(dispatch_count, 1, 1);
            stride <<= 1;
            iteration += 1;
        }

        cpass.set_pipeline(&cull_pass.post_cull_pipeline);
        cpass.set_push_constants(0, &[data.object_count]);
        cpass.set_bind_group(0, object_input_bg, &[]);
        cpass.set_bind_group(1, &data.inner.as_gpu().output_bg, &[]);
        cpass.set_bind_group(2, uniform_bg, &[]);
        cpass.dispatch(dispatch_count, 1, 1);
    }
}
