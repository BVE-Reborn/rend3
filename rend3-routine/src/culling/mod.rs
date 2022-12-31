use std::{
    any::{type_name, TypeId},
    borrow::Cow,
    collections::HashMap,
    iter::zip,
    num::NonZeroU64,
    ops::Range,
};

use encase::{ShaderSize, ShaderType, StorageBuffer};
use rend3::{
    format_sso,
    graph::{DataHandle, NodeResourceUsage, RenderGraph},
    managers::{MaterialManager, ObjectManager, ShaderObject, TextureBindGroupIndex},
    types::Material,
    util::math::{round_up, round_up_div},
    Renderer, ShaderPreProcessor, ShaderVertexBufferConfig,
};
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    Buffer, BufferBinding, BufferBindingType, BufferDescriptor, BufferUsages, CommandEncoder, ComputePassDescriptor,
    ComputePipeline, ComputePipelineDescriptor, PipelineLayoutDescriptor, ShaderModuleDescriptor, ShaderStages,
};

const BATCH_SIZE: usize = 256;
const WORKGROUP_SIZE: u32 = 256;

#[derive(Debug)]
pub struct ShaderBatchDatas {
    keys: Vec<ShaderJobKey>,
    jobs: Vec<ShaderBatchData>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ShaderJobKey {
    material_key: u64,
    bind_group_index: TextureBindGroupIndex,
}

#[derive(Debug, ShaderType)]
pub struct ShaderBatchData {
    #[align(256)]
    ranges: [ShaderObjectRange; BATCH_SIZE],
    total_objects: u32,
    total_invocations: u32,
    base_output_invocation: u32,
}

#[derive(Debug, Copy, Clone, Default, ShaderType)]
struct ShaderObjectRange {
    invocation_start: u32,
    invocation_end: u32,
    object_id: u32,
}

fn batch_objects<M: Material>(material_manager: &MaterialManager, object_manager: &ObjectManager) -> ShaderBatchDatas {
    profiling::scope!("Batch Objects");

    let mut jobs = ShaderBatchDatas {
        jobs: Vec::new(),
        keys: Vec::new(),
    };

    let objects = match object_manager.enumerated_objects::<M>() {
        Some(o) => o,
        None => return jobs,
    };

    let material_archetype = material_manager.archetype_view::<M>();

    let mut sorted_objects = Vec::with_capacity(objects.len());
    {
        profiling::scope!("Sort Key Creation");
        for (handle, object) in objects {
            let material = material_archetype.material(*object.material_handle);
            let bind_group_index = material
                .bind_group_index
                .map_gpu(|_| TextureBindGroupIndex::DUMMY)
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
    }

    {
        profiling::scope!("Sorting");
        sorted_objects.sort_unstable_by_key(|(k, _, _)| *k);
    }

    if !sorted_objects.is_empty() {
        profiling::scope!("Batch Data Creation");
        let mut current_base_invocation = 0_u32;
        let mut current_invocation = 0_u32;
        let mut current_object_index = 0_u32;
        let mut current_ranges = [ShaderObjectRange::default(); BATCH_SIZE];
        let mut current_key = sorted_objects.first().unwrap().0;

        for (key, handle, object) in sorted_objects {
            if key != current_key || current_object_index == 256 {
                jobs.jobs.push(ShaderBatchData {
                    ranges: current_ranges,
                    total_objects: current_object_index,
                    total_invocations: current_invocation,
                    base_output_invocation: current_base_invocation,
                });
                jobs.keys.push(current_key);

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
            current_invocation += round_up(invocation_count, WORKGROUP_SIZE);
        }

        jobs.jobs.push(ShaderBatchData {
            ranges: current_ranges,
            total_objects: current_object_index,
            total_invocations: current_invocation,
            base_output_invocation: current_base_invocation,
        });
        jobs.keys.push(current_key);
    }

    jobs
}

#[derive(Debug)]
pub struct DrawCallSet {
    pub object_reference_buffer: Buffer,
    pub index_buffer: Buffer,
    pub draw_calls: Vec<DrawCall>,
    /// Range of draw calls in the draw call array corresponding to a given material key.
    pub material_key_ranges: HashMap<u64, Range<usize>>,
}

#[derive(Debug)]
pub struct DrawCall {
    pub bind_group_index: TextureBindGroupIndex,
    pub index_range: Range<u32>,
}

pub struct GpuCuller {
    bgl: BindGroupLayout,
    pipeline: ComputePipeline,
    type_id: TypeId,
}

impl GpuCuller {
    pub fn new<M>(renderer: &Renderer, spp: &ShaderPreProcessor) -> Self
    where
        M: Material,
    {
        let type_name = type_name::<M>();

        let source = spp
            .render_shader(
                "rend3-routine/cull.wgsl",
                &(),
                Some(&ShaderVertexBufferConfig::from_material::<M>()),
            )
            .unwrap();

        let sm = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} SM")),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(source)),
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
                        min_binding_size: Some(ShaderBatchData::SHADER_SIZE),
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(4).unwrap()),
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
            type_id: TypeId::of::<M>(),
        }
    }

    pub fn cull<M>(
        &self,
        renderer: &Renderer,
        encoder: &mut CommandEncoder,
        jobs: ShaderBatchDatas,
        vertex_data_buffer: &Buffer,
        object_data_buffer: &Buffer,
    ) -> DrawCallSet
    where
        M: Material,
    {
        profiling::scope!("GpuCuller::cull");

        assert_eq!(TypeId::of::<M>(), self.type_id);

        let type_name = type_name::<M>();

        let object_reference_buffer = renderer.device.create_buffer(&BufferDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Object Reference Buffer")),
            size: jobs.jobs.size().get(),
            usage: BufferUsages::STORAGE,
            mapped_at_creation: true,
        });
        {
            profiling::scope!("Culling Job Data Upload");
            StorageBuffer::new(&mut *object_reference_buffer.slice(..).get_mapped_range_mut())
                .write(&jobs.jobs)
                .unwrap();
        }
        object_reference_buffer.unmap();

        let total_invocations: u32 = jobs
            .jobs
            .iter()
            .map(|j: &ShaderBatchData| {
                debug_assert_eq!(j.total_invocations % 256, 0);
                j.total_invocations
            })
            .sum();
        let output_buffer = renderer.device.create_buffer(&BufferDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Output Buffer")),
            size: (total_invocations as u64 * 3 * 4).max(4),
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
                    resource: wgpu::BindingResource::Buffer(BufferBinding {
                        buffer: &object_reference_buffer,
                        offset: 0,
                        size: Some(ShaderBatchData::SHADER_SIZE),
                    }),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: output_buffer.as_entire_binding(),
                },
            ],
        });

        profiling::scope!("Command Encoding");
        let mut draw_calls = Vec::with_capacity(jobs.jobs.len());
        let mut material_key_ranges = HashMap::new();

        let mut current_material_key_range_start = 0;
        let mut current_material_key = jobs.keys.first().map(|k| k.material_key).unwrap_or(0);

        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Culling")),
        });
        cpass.set_pipeline(&self.pipeline);
        for (idx, (key, job)) in zip(jobs.keys, jobs.jobs).enumerate() {
            // RA is having a lot of trouble with into_iter.
            let (key, job): (ShaderJobKey, ShaderBatchData) = (key, job);

            if current_material_key != key.material_key {
                let range_end = draw_calls.len();
                material_key_ranges.insert(current_material_key, current_material_key_range_start..range_end);
                current_material_key = key.material_key;
                current_material_key_range_start = range_end;
            }

            cpass.set_bind_group(0, &bg, &[idx as u32 * ShaderBatchData::SHADER_SIZE.get() as u32]);
            cpass.dispatch_workgroups(round_up_div(job.total_invocations, WORKGROUP_SIZE), 1, 1);

            draw_calls.push(DrawCall {
                index_range: (job.base_output_invocation * 3)..(job.base_output_invocation + job.total_invocations) * 3,
                bind_group_index: key.bind_group_index,
            });
        }
        drop(cpass);

        material_key_ranges.insert(current_material_key, current_material_key_range_start..draw_calls.len());

        DrawCallSet {
            object_reference_buffer,
            index_buffer: output_buffer,
            draw_calls,
            material_key_ranges,
        }
    }
}

pub fn add_culling_to_graph<'node, M: Material>(
    graph: &mut RenderGraph<'node>,
    draw_calls_hdl: DataHandle<DrawCallSet>,
    culler: &'node GpuCuller,
    name: &str,
) {
    let mut node = graph.add_node(name);
    let output = node.add_data(draw_calls_hdl, NodeResourceUsage::Output);

    node.build(move |ctx| {
        let jobs = batch_objects::<M>(&ctx.data_core.material_manager, &ctx.data_core.object_manager);

        if jobs.jobs.is_empty() {
            return;
        }

        let encoder = ctx.encoder_or_pass.get_encoder();
        let draw_calls = culler.cull::<M>(
            ctx.renderer,
            encoder,
            jobs,
            ctx.data_core.mesh_manager.buffer(),
            ctx.data_core.object_manager.buffer::<M>().unwrap(),
        );

        ctx.graph_data.set_data(output, Some(draw_calls));
    });
}
