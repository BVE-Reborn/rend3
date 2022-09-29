use std::{any::type_name, marker::PhantomData, num::NonZeroU64, ops::Range};

use encase::{ShaderSize, ShaderType, StorageBuffer};
use rend3::{
    format_sso,
    managers::{MaterialManager, ObjectManager, ShaderObject, TextureBindGroupIndex},
    types::{Material, MaterialArray},
    util::math::round_up_pot,
    Renderer, ShaderPreProcessor,
};
use serde::Serialize;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    Buffer, BufferBindingType, BufferDescriptor, BufferUsages, CommandEncoder, ComputePassDescriptor, ComputePipeline,
    ComputePipelineDescriptor, PipelineLayoutDescriptor, ShaderModuleDescriptor, ShaderStages,
};

const BATCH_SIZE: usize = 256;
const WORKGROUP_SIZE: u32 = 256;

struct ShaderCullingJobs {
    keys: Vec<ShaderJobKey>,
    jobs: Vec<ShaderCullingJob>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ShaderJobKey {
    material_key: u64,
    bind_group_index: TextureBindGroupIndex,
}

#[derive(ShaderType)]
struct ShaderCullingJob {
    #[align(256)]
    ranges: [ShaderObjectRange; BATCH_SIZE],
    total_objects: u32,
    total_invocations: u32,
    base_output_invocation: u32,
}

#[derive(Copy, Clone, Default, ShaderType)]
struct ShaderObjectRange {
    invocation_start: u32,
    invocation_end: u32,
    object_id: u32,
}

fn batch_objects<M: Material>(material_manager: &MaterialManager, object_manager: &ObjectManager) -> ShaderCullingJobs {
    let objects = object_manager.enumerated_objects::<M>();
    let predicted_count = objects.size_hint().1.unwrap_or(0);

    let material_archetype = material_manager.archetype_view::<M>();

    let mut sorted_objects = Vec::with_capacity(predicted_count);
    for (handle, object) in objects {
        let material = material_archetype.material(*object.material_handle);
        let bind_group_index = material
            .bind_group_index
            .map_gpu(|| TextureBindGroupIndex::DUMMY)
            .into_common();

        let material_key = material.inner.key();

        sorted_objects.push((
            ShaderJobKey {
                material_key,
                bind_group_index,
            },
            handle,
            object,
        ))
    }

    sorted_objects.sort_unstable_by_key(|(k, _, _)| k);

    let jobs = ShaderCullingJobs {
        jobs: Vec::new(),
        keys: Vec::new(),
    };

    if !sorted_objects.is_empty() {
        let mut current_base_invocation = 0_u32;
        let mut current_invocation = 0_u32;
        let mut current_object_index = 0_u32;
        let mut current_ranges = [ShaderObjectRange::default(); BATCH_SIZE];
        let mut current_key = sorted_objects.first().unwrap().0;

        for (key, handle, object) in sorted_objects {
            if key != current_key {
                jobs.jobs.push(ShaderCullingJob {
                    ranges: current_ranges,
                    total_objects: current_object_index,
                    total_invocations: current_invocation,
                    base_output_invocation: current_base_invocation,
                });
                jobs.keys.push(key);

                current_base_invocation += current_invocation;
                current_key = key;
                current_invocation = 0;
                current_object_index = 0;
            }

            let invocation_count = object.inner.index_count / 3;
            let range = ShaderObjectRange {
                invocation_start: current_invocation,
                invocation_end: current_invocation + invocation_count,
                object_id: handle.idx as u32,
            };

            current_ranges[current_object_index as usize] = range;
            current_object_index += 1;
            current_invocation += round_up_pot(invocation_count, WORKGROUP_SIZE);
        }
    }

    jobs
}

struct DrawCallSet {
    object_data_buffer: Buffer,
    index_buffer: Buffer,
    calls: Vec<DrawCall>,
}

struct DrawCall {
    index_range: Range<u32>,
}

#[derive(Serialize)]
struct CullingPreprocessingArguments {
    vertex_array_counts: u32,
}

struct GpuCuller<M> {
    bgl: BindGroupLayout,
    pipeline: ComputePipeline,
    _phantom: PhantomData<M>,
}

impl<M> GpuCuller<M>
where
    M: Material,
{
    pub fn new(renderer: &Renderer, spp: &ShaderPreProcessor) {
        let type_name = type_name::<M>();

        let source = spp
            .render_shader(
                "base",
                &CullingPreprocessingArguments {
                    vertex_array_counts: <M::SupportedAttributeArrayType as MaterialArray>::COUNT,
                },
            )
            .unwrap();

        let sm = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} SM")),
            source: todo!(),
        });

        let bgl = renderer.device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} BGL")),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: NonZeroU64::new(4),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(ShaderObject::<M>::SHADER_SIZE),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: true,
                        min_binding_size: Some(ShaderCullingJob::SHADER_SIZE),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(4)),
                    },
                    count: None,
                },
            ],
        });

        let pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} PLL")),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = renderer.device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} PLL")),
            layout: Some(&pll),
            module: &sm,
            entry_point: "cs_main",
        });

        Self {
            bgl,
            pipeline,
            _phantom: PhantomData,
        }
    }

    pub fn cull(
        &self,
        renderer: &Renderer,
        encoder: &mut CommandEncoder,
        jobs: ShaderCullingJobs,
        vertex_data_buffer: &Buffer,
        object_data_buffer: &Buffer,
    ) -> DrawCallSet {
        let type_name = type_name::<M>();

        let job_buffer = renderer.device.create_buffer(&BufferDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Job Buffer")),
            size: jobs.size().get(),
            usage: BufferUsages::STORAGE,
            mapped_at_creation: true,
        });
        StorageBuffer::new(&mut *job_buffer.slice(..).get_mapped_range_mut()).write(&jobs.jobs);
        job_buffer.unmap();

        let total_invocations: u32 = jobs
            .jobs
            .iter()
            .map(|j: &ShaderCullingJob| {
                debug_assert_eq!(j.total_invocations % 256, 0);
                j.total_invocations
            })
            .sum();
        let output_buffer = renderer.device.create_buffer(&BufferDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Output Buffer")),
            size: total_invocations * 3,
            usage: BufferUsages::STORAGE | BufferUsages::INDEX,
            mapped_at_creation: false,
        });

        let bg = renderer.device.create_bind_group(&BindGroupDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} BG")),
            layout: &self.bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: vertex_data_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: object_data_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: job_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        let cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Culling")),
        });
        cpass.set_pipeline(&self.pipeline);
        for (idx, job) in jobs.jobs.into_iter().enumerate() {
            let job: ShaderCullingJob = job;

            cpass.set_bind_group(0, &bg, &[idx * ShaderCullingJob::SHADER_SIZE]);
            cpass.dispatch_workgroups(job.total_invocations / WORKGROUP_SIZE, 1, 1);
        }
        drop(cpass);
    }
}
