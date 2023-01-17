use std::{
    any::{type_name, TypeId},
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    mem,
    num::NonZeroU64,
    ops::Range,
    sync::Arc,
};

use encase::{ShaderSize, ShaderType, StorageBuffer};
use rend3::{
    format_sso,
    graph::{DataHandle, NodeExecutionContext, NodeResourceUsage, RenderGraph},
    managers::{ShaderObject, TextureBindGroupIndex},
    types::{GraphDataHandle, Material},
    util::{
        math::{round_up, round_up_div},
        typedefs::FastHashMap,
    },
    Renderer, ShaderPreProcessor, ShaderVertexBufferConfig,
};
use wgpu::{
    self, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingType, Buffer, BufferBinding, BufferBindingType, BufferDescriptor, BufferUsages, CommandEncoder,
    ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor,
    ShaderModuleDescriptor, ShaderStages,
};

use crate::culling::{
    batching::{batch_objects, JobSubRegion, ShaderBatchData, ShaderBatchDatas},
    WORKGROUP_SIZE,
};

// 16 MB of indices
const OUTPUT_BUFFER_ROUNDING_SIZE: u64 = 1 << 24;
// At least 64 batches
const BATCH_DATA_ROUNDING_SIZE: u64 = ShaderBatchData::SHADER_SIZE.get() * 64;

#[derive(Debug)]
pub struct DrawCallSet {
    pub buffers: CullingBuffers<Arc<Buffer>>,
    pub draw_calls: Vec<DrawCall>,
    /// Range of draw calls in the draw call array corresponding to a given material key.
    pub material_key_ranges: HashMap<u64, Range<usize>>,
}

#[derive(Debug)]
pub struct DrawCall {
    pub bind_group_index: TextureBindGroupIndex,
    pub batch_index: u32,
}

#[derive(Default)]
struct CullingBufferMap {
    inner: FastHashMap<Option<usize>, CullingBuffers<Arc<Buffer>>>,
}
impl CullingBufferMap {
    fn get_buffers(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        camera: Option<usize>,
        mut sizes: CullingBuffers<u64>,
    ) -> &CullingBuffers<Arc<Buffer>> {
        sizes.object_reference = round_up(sizes.object_reference.max(1), BATCH_DATA_ROUNDING_SIZE);
        sizes.primary_index = round_up(sizes.primary_index.max(1), OUTPUT_BUFFER_ROUNDING_SIZE);

        match self.inner.entry(camera) {
            Entry::Occupied(b) => {
                let b = b.into_mut();

                // We swap previous and current, and make it so that the previous culling results
                // never need to change size. All size changes "start" with the current and then
                // propogate back.
                mem::swap(&mut b.previous_culling_results, &mut b.current_culling_results);
                sizes.previous_culling_results = b.previous_culling_results.size();

                let current_size = CullingBuffers {
                    object_reference: b.object_reference.size(),
                    primary_index: b.primary_index.size(),
                    secondary_index: b.secondary_index.size(),
                    primary_draw_call: b.primary_draw_call.size(),
                    secondary_draw_call: b.secondary_draw_call.size(),
                    previous_culling_results: b.previous_culling_results.size(),
                    current_culling_results: b.current_culling_results.size(),
                };
                if current_size != sizes {
                    let old_bufs = mem::replace(&mut *b, CullingBuffers::new(device, sizes));
                    encoder.copy_buffer_to_buffer(
                        &old_bufs.previous_culling_results,
                        0,
                        &b.previous_culling_results,
                        0,
                        current_size.previous_culling_results,
                    )
                }
                b
            }
            Entry::Vacant(b) => b.insert(CullingBuffers::new(device, sizes)),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CullingBuffers<T> {
    pub object_reference: T,
    pub primary_index: T,
    pub secondary_index: T,
    pub primary_draw_call: T,
    pub secondary_draw_call: T,
    pub previous_culling_results: T,
    pub current_culling_results: T,
}

impl CullingBuffers<Arc<Buffer>> {
    pub fn new(device: &Device, sizes: CullingBuffers<u64>) -> Self {
        CullingBuffers {
            object_reference: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.object_reference,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })),
            primary_index: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.primary_index,
                usage: BufferUsages::STORAGE | BufferUsages::INDEX,
                mapped_at_creation: false,
            })),
            secondary_index: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.secondary_index,
                usage: BufferUsages::STORAGE | BufferUsages::INDEX,
                mapped_at_creation: false,
            })),
            primary_draw_call: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.primary_draw_call,
                usage: BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })),
            secondary_draw_call: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.secondary_draw_call,
                usage: BufferUsages::STORAGE | BufferUsages::INDIRECT | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })),
            previous_culling_results: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.previous_culling_results,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })),
            current_culling_results: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.current_culling_results,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })),
        }
    }
}

pub struct GpuCuller {
    bgl: BindGroupLayout,
    pipeline: ComputePipeline,
    type_id: TypeId,
    culling_buffer_map_handle: GraphDataHandle<CullingBufferMap>,
}

impl GpuCuller {
    pub fn new<M>(renderer: &Arc<Renderer>, spp: &ShaderPreProcessor) -> Self
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
                // Vertex
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
                // Object
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
                // Batch data
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
                // Primary draw calls
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(20).unwrap()),
                    },
                    count: None,
                },
                // Secondary draw calls
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(20).unwrap()),
                    },
                    count: None,
                },
                // Primary indices
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                    },
                    count: None,
                },
                // secondary indices
                BindGroupLayoutEntry {
                    binding: 6,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                    },
                    count: None,
                },
                // previous culling results
                BindGroupLayoutEntry {
                    binding: 7,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(4).unwrap()),
                    },
                    count: None,
                },
                // current culling results
                BindGroupLayoutEntry {
                    binding: 8,
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

        let culling_buffer_map_handle = renderer.add_graph_data(CullingBufferMap::default());

        Self {
            bgl,
            pipeline,
            type_id: TypeId::of::<M>(),
            culling_buffer_map_handle,
        }
    }

    pub fn cull<M>(&self, ctx: &mut NodeExecutionContext, jobs: ShaderBatchDatas, camera: Option<usize>) -> DrawCallSet
    where
        M: Material,
    {
        profiling::scope!("GpuCuller::cull");

        assert_eq!(TypeId::of::<M>(), self.type_id);

        let type_name = type_name::<M>();

        let total_invocations: u32 = jobs
            .jobs
            .iter()
            .map(|j: &ShaderBatchData| {
                debug_assert_eq!(j.total_invocations % 256, 0);
                j.total_invocations
            })
            .sum();

        let encoder = ctx.encoder_or_pass.take_encoder();

        let buffers = ctx
            .data_core
            .graph_storage
            .get_mut(&self.culling_buffer_map_handle)
            .get_buffers(
                &ctx.renderer.device,
                encoder,
                camera,
                CullingBuffers {
                    object_reference: jobs.jobs.size().get(),
                    // RA is getting totally weird with the call to max, thinking it's a call to Iter::max
                    // this makes the errors go away.
                    primary_index: <u64 as Ord>::max(total_invocations as u64 * 3 * 4, 4),
                    secondary_index: <u64 as Ord>::max(total_invocations as u64 * 3 * 4, 4),
                    primary_draw_call: <u64 as Ord>::max(jobs.regions.len() as u64 * 20, 20),
                    secondary_draw_call: <u64 as Ord>::max(jobs.regions.len() as u64 * 20, 20),
                    current_culling_results: <u64 as Ord>::max(total_invocations as u64 / 8, 4),
                    previous_culling_results: <u64 as Ord>::max(total_invocations as u64 / 8, 4),
                },
            )
            .clone();

        {
            profiling::scope!("Culling Job Data Format");
            let mut buffer = ctx
                .renderer
                .queue
                .write_buffer_with(&buffers.object_reference, 0, jobs.jobs.size())
                .unwrap();
            StorageBuffer::new(&mut *buffer).write(&jobs.jobs).unwrap();
        }

        let bg = ctx.renderer.device.create_bind_group(&BindGroupDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} BG")),
            layout: &self.bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: ctx.eval_output.mesh_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: ctx.data_core.object_manager.buffer::<M>().unwrap().as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Buffer(BufferBinding {
                        buffer: &buffers.object_reference,
                        offset: 0,
                        size: Some(ShaderBatchData::SHADER_SIZE),
                    }),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: buffers.primary_draw_call.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: buffers.secondary_draw_call.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: buffers.primary_index.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: buffers.secondary_index.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: buffers.previous_culling_results.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 8,
                    resource: buffers.current_culling_results.as_entire_binding(),
                },
            ],
        });

        profiling::scope!("Command Encoding");
        let mut draw_calls = Vec::with_capacity(jobs.jobs.len());
        let mut material_key_ranges = HashMap::new();

        let mut current_material_key_range_start = 0;
        let mut current_material_key = jobs.regions.first().map(|k| k.key.material_key).unwrap_or(0);
        for region in jobs.regions {
            let region: JobSubRegion = region;

            if current_material_key != region.key.material_key {
                let range_end = draw_calls.len();
                material_key_ranges.insert(current_material_key, current_material_key_range_start..range_end);
                current_material_key = region.key.material_key;
                current_material_key_range_start = range_end;
            }

            draw_calls.push(DrawCall {
                bind_group_index: region.key.bind_group_index,
                batch_index: region.job_index,
            });
        }

        material_key_ranges.insert(current_material_key, current_material_key_range_start..draw_calls.len());

        // TODO: this is needed to zero out the indirect vertex count, this could be improved.
        encoder.clear_buffer(&buffers.primary_draw_call, 0, None);
        encoder.clear_buffer(&buffers.secondary_draw_call, 0, None);
        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Culling")),
        });
        cpass.set_pipeline(&self.pipeline);
        for (idx, job) in jobs.jobs.iter().enumerate() {
            cpass.set_bind_group(0, &bg, &[idx as u32 * ShaderBatchData::SHADER_SIZE.get() as u32]);
            cpass.dispatch_workgroups(round_up_div(job.total_invocations, WORKGROUP_SIZE), 1, 1);
        }
        drop(cpass);

        DrawCallSet {
            buffers,
            draw_calls,
            material_key_ranges,
        }
    }

    pub fn add_culling_to_graph<'node, M: Material>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        draw_calls_hdl: DataHandle<DrawCallSet>,
        camera: Option<usize>,
        name: &str,
    ) {
        let mut node = graph.add_node(name);
        let output = node.add_data(draw_calls_hdl, NodeResourceUsage::Output);

        node.build(move |mut ctx| {
            let camera_manager = match camera {
                Some(i) => &ctx.eval_output.shadows[i].camera,
                None => &ctx.data_core.camera_manager,
            };

            let jobs = batch_objects::<M>(
                &ctx.data_core.material_manager,
                &ctx.data_core.object_manager,
                camera_manager,
                ctx.renderer.limits.max_compute_workgroups_per_dimension,
            );

            if jobs.jobs.is_empty() {
                return;
            }

            let draw_calls = self.cull::<M>(&mut ctx, jobs, camera);

            ctx.graph_data.set_data(output, Some(draw_calls));
        });
    }
}
