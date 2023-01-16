use std::{
    any::{type_name, TypeId},
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
    iter::zip,
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
    BindingType, Buffer, BufferBinding, BufferBindingType, BufferDescriptor, BufferUsages, ComputePassDescriptor,
    ComputePipeline, ComputePipelineDescriptor, Device, PipelineLayoutDescriptor, ShaderModuleDescriptor, ShaderStages,
};

use crate::culling::{
    batching::{batch_objects, ShaderBatchData, ShaderBatchDatas, ShaderJobKey},
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
    pub index_range: Range<u32>,
}

#[derive(Default)]
struct CullingBufferMap {
    inner: FastHashMap<Option<usize>, CullingBuffers<Arc<Buffer>>>,
}
impl CullingBufferMap {
    fn get_buffers(
        &mut self,
        device: &Device,
        camera: Option<usize>,
        mut sizes: CullingBuffers<u64>,
    ) -> &CullingBuffers<Arc<Buffer>> {
        sizes.object_reference = round_up(sizes.object_reference.max(1), BATCH_DATA_ROUNDING_SIZE);
        sizes.index = round_up(sizes.index.max(1), OUTPUT_BUFFER_ROUNDING_SIZE);

        match self.inner.entry(camera) {
            Entry::Occupied(b) => {
                let b = b.into_mut();

                let current_size = CullingBuffers {
                    object_reference: b.object_reference.size(),
                    index: b.index.size(),
                };
                if current_size != sizes {
                    *b = CullingBuffers::new(device, sizes);
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
    pub index: T,
}

impl CullingBuffers<Arc<Buffer>> {
    pub fn new(device: &Device, sizes: CullingBuffers<u64>) -> Self {
        CullingBuffers {
            object_reference: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.object_reference,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })),
            index: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.index,
                usage: BufferUsages::STORAGE | BufferUsages::INDEX,
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

        let buffers = ctx
            .data_core
            .graph_storage
            .get_mut(&self.culling_buffer_map_handle)
            .get_buffers(
                &ctx.renderer.device,
                camera,
                CullingBuffers {
                    object_reference: jobs.jobs.size().get(),
                    index: <u64 as Ord>::max(total_invocations as u64 * 3 * 4, 4),
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
                    resource: buffers.index.as_entire_binding(),
                },
            ],
        });

        profiling::scope!("Command Encoding");
        let mut draw_calls = Vec::with_capacity(jobs.jobs.len());
        let mut material_key_ranges = HashMap::new();

        let mut current_material_key_range_start = 0;
        let mut current_material_key = jobs.keys.first().map(|k| k.material_key).unwrap_or(0);

        let mut cpass = ctx
            .encoder_or_pass
            .take_encoder()
            .begin_compute_pass(&ComputePassDescriptor {
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
            );

            if jobs.jobs.is_empty() {
                return;
            }

            let draw_calls = self.cull::<M>(&mut ctx, jobs, camera);

            ctx.graph_data.set_data(output, Some(draw_calls));
        });
    }
}
