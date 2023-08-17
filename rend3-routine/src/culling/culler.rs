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
use glam::{Mat4, UVec2, Vec2};
use rend3::{
    format_sso,
    graph::{DataHandle, DeclaredDependency, NodeExecutionContext, NodeResourceUsage, RenderGraph, RenderTargetHandle},
    managers::{CameraManager, ShaderObject, TextureBindGroupIndex},
    types::{GraphDataHandle, Material, MaterialArray, RawObjectHandle, SampleCount, VERTEX_ATTRIBUTE_POSITION},
    util::{
        frustum::Frustum,
        math::{round_up, round_up_div},
        typedefs::FastHashMap,
    },
    Renderer, ShaderPreProcessor, ShaderVertexBufferConfig,
};
use wgpu::{
    self, AddressMode, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBinding, BufferBindingType, BufferDescriptor,
    BufferUsages, CommandEncoder, ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device,
    FilterMode, PipelineLayoutDescriptor, Sampler, SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor,
    ShaderStages, TextureSampleType, TextureViewDimension,
};

use crate::culling::{
    batching::{batch_objects, JobSubRegion, ShaderBatchData, ShaderBatchDatas},
    WORKGROUP_SIZE,
};

// 16 MB of indices
const OUTPUT_BUFFER_ROUNDING_SIZE: u64 = 1 << 24;
// At least 64 batches
const BATCH_DATA_ROUNDING_SIZE: u64 = ShaderBatchData::SHADER_SIZE.get() * 64;

#[derive(Debug, Clone)]
pub struct DrawCallSet {
    pub per_camera_uniform: Arc<Buffer>,
    pub buffers: CullingBuffers<Arc<Buffer>>,
    pub draw_calls: Vec<DrawCall>,
    /// Range of draw calls in the draw call array corresponding to a given material key.
    pub material_key_ranges: HashMap<u64, Range<usize>>,
}

#[derive(Debug, Clone)]
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
        sizes.current_object_reference = round_up(sizes.current_object_reference.max(1), BATCH_DATA_ROUNDING_SIZE);
        sizes.primary_index = round_up(sizes.primary_index.max(1), OUTPUT_BUFFER_ROUNDING_SIZE);

        match self.inner.entry(camera) {
            Entry::Occupied(b) => {
                let b = b.into_mut();

                // We swap previous and current, and make it so that the previous culling results
                // never need to change size. All size changes "start" with the current and then
                // propogate back.
                mem::swap(&mut b.previous_culling_results, &mut b.current_culling_results);
                sizes.previous_culling_results = b.previous_culling_results.size();
                mem::swap(&mut b.previous_object_reference, &mut b.current_object_reference);
                sizes.previous_object_reference = b.previous_object_reference.size();

                let current_size = CullingBuffers {
                    previous_object_reference: b.previous_object_reference.size(),
                    current_object_reference: b.current_object_reference.size(),
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
                    );
                    encoder.copy_buffer_to_buffer(
                        &old_bufs.previous_object_reference,
                        0,
                        &b.previous_object_reference,
                        0,
                        current_size.previous_object_reference,
                    );
                }
                b
            }
            Entry::Vacant(b) => b.insert(CullingBuffers::new(device, sizes)),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CullingBuffers<T> {
    pub previous_object_reference: T,
    pub current_object_reference: T,
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
            previous_object_reference: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.previous_object_reference,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })),
            current_object_reference: Arc::new(device.create_buffer(&BufferDescriptor {
                label: None,
                size: sizes.current_object_reference,
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

#[derive(Debug, Copy, Clone)]
pub enum TriangleVisibility {
    PositiveAreaVisible,
    NegativeAreaVisible,
}

impl TriangleVisibility {
    fn from_winding_and_face(winding: wgpu::FrontFace, culling: wgpu::Face) -> Self {
        match (winding, culling) {
            (wgpu::FrontFace::Ccw, wgpu::Face::Back) => TriangleVisibility::PositiveAreaVisible,
            (wgpu::FrontFace::Ccw, wgpu::Face::Front) => TriangleVisibility::NegativeAreaVisible,
            (wgpu::FrontFace::Cw, wgpu::Face::Back) => TriangleVisibility::NegativeAreaVisible,
            (wgpu::FrontFace::Cw, wgpu::Face::Front) => TriangleVisibility::PositiveAreaVisible,
        }
    }

    fn is_positive(self) -> bool {
        match self {
            TriangleVisibility::PositiveAreaVisible => true,
            TriangleVisibility::NegativeAreaVisible => false,
        }
    }
}

bitflags::bitflags! {
    struct PerCameraUniformFlags: u32 {
        const POSTIIVE_AREA_VISIBLE = 1 << 0;
        const MULTISAMPLED = 1 << 1;
    }
}

#[derive(ShaderType)]
struct PerCameraUniform {
    // TODO: use less space
    view: Mat4,
    // TODO: use less space
    view_proj: Mat4,
    // The index of which shadow caster we are rendering for.
    //
    // This will be u32::MAX if we're rendering for a camera, not a shadow map.
    shadow_index: u32,
    frustum: Frustum,
    resolution: Vec2,
    // Created from PerCameraUniformFlags
    flags: u32,
    object_count: u32,
    #[size(runtime)]
    objects: Vec<PerCameraUniformObjectData>,
}

#[derive(ShaderType)]
struct PerCameraUniformObjectData {
    // TODO: use less space
    model_view: Mat4,
    // TODO: use less space
    model_view_proj: Mat4,
}

pub struct GpuCuller {
    prep_bgl: BindGroupLayout,
    prep_pipeline: ComputePipeline,
    culling_bgl: BindGroupLayout,
    culling_pipeline: ComputePipeline,
    sampler: Sampler,
    winding: wgpu::FrontFace,
    type_id: TypeId,
    per_material_buffer_handle: GraphDataHandle<HashMap<Option<usize>, Arc<Buffer>>>,
    culling_buffer_map_handle: GraphDataHandle<CullingBufferMap>,
    previous_invocation_map_handle: GraphDataHandle<HashMap<Option<usize>, HashMap<RawObjectHandle, u32>>>,
}

impl GpuCuller {
    pub fn new<M>(renderer: &Arc<Renderer>, spp: &ShaderPreProcessor) -> Self
    where
        M: Material,
    {
        let type_name = type_name::<M>();

        let prep_source = spp
            .render_shader(
                "rend3-routine/uniform_prep.wgsl",
                &(),
                Some(&ShaderVertexBufferConfig::from_material::<M>()),
            )
            .unwrap();

        let prep_sm = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some(&format_sso!("UniformPrep {type_name} SM")),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(prep_source)),
        });

        let prep_bgl = renderer.device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some(&format_sso!("UniformPrep {type_name} BGL")),
            entries: &[
                // Object
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(ShaderObject::<M>::SHADER_SIZE),
                    },
                    count: None,
                },
                // Object
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(PerCameraUniform::min_size()),
                    },
                    count: None,
                },
            ],
        });

        let prep_pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some(&format_sso!("UniformPrep {type_name} PLL")),
            bind_group_layouts: &[&prep_bgl],
            push_constant_ranges: &[],
        });

        let prep_pipeline = renderer.device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some(&format_sso!("UniformPrep {type_name} PLL")),
            layout: Some(&prep_pll),
            module: &prep_sm,
            entry_point: "cs_main",
        });

        let position_offset = M::supported_attributes()
            .into_iter()
            .enumerate()
            .find_map(|(idx, a)| (*a == *VERTEX_ATTRIBUTE_POSITION).then_some(idx))
            .unwrap();

        let culling_source = spp
            .render_shader(
                "rend3-routine/cull.wgsl",
                &{
                    let mut map = HashMap::new();
                    map.insert("position_attribute_offset", position_offset);
                    map
                },
                Some(&ShaderVertexBufferConfig::from_material::<M>()),
            )
            .unwrap();

        let culling_sm = renderer.device.create_shader_module(ShaderModuleDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} SM")),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(culling_source)),
        });

        let culling_bgl = renderer.device.create_bind_group_layout(&BindGroupLayoutDescriptor {
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
                // per camera object data
                BindGroupLayoutEntry {
                    binding: 9,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: Some(PerCameraUniform::min_size()),
                    },
                    count: None,
                },
                // hirearchical z buffer
                BindGroupLayoutEntry {
                    binding: 10,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Depth,
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // hirearchical z buffer
                BindGroupLayoutEntry {
                    binding: 11,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Sampler(SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });

        let culling_pll = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} PLL")),
            bind_group_layouts: &[&culling_bgl],
            push_constant_ranges: &[],
        });

        let culling_pipeline = renderer.device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} PLL")),
            layout: Some(&culling_pll),
            module: &culling_sm,
            entry_point: "cs_main",
        });

        let sampler = renderer.device.create_sampler(&SamplerDescriptor {
            label: Some("HiZ Sampler"),
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            compare: None,
            anisotropy_clamp: 1,
            border_color: None,
        });

        let per_material_buffer_handle = renderer.add_graph_data(HashMap::default());
        let culling_buffer_map_handle = renderer.add_graph_data(CullingBufferMap::default());
        let previous_invocation_map_handle = renderer.add_graph_data(HashMap::default());

        Self {
            prep_bgl,
            prep_pipeline,
            culling_bgl,
            culling_pipeline,
            sampler,
            winding: renderer.handedness.into(),
            type_id: TypeId::of::<M>(),
            per_material_buffer_handle,
            culling_buffer_map_handle,
            previous_invocation_map_handle,
        }
    }

    pub fn uniform_bake<M>(
        &self,
        ctx: &mut NodeExecutionContext,
        camera: &CameraManager,
        camera_idx: Option<usize>,
        resolution: UVec2,
        samples: SampleCount,
    ) where
        M: Material,
    {
        profiling::scope!("GpuCuller::uniform_bake");

        assert_eq!(TypeId::of::<M>(), self.type_id);

        let type_name = type_name::<M>();

        let encoder = ctx.encoder_or_pass.take_encoder();

        // TODO: Isolate all this into a struct
        let max_object_count = ctx
            .data_core
            .object_manager
            .buffer::<M>()
            .map(wgpu::Buffer::size)
            .unwrap_or(0)
            / ShaderObject::<M>::SHADER_SIZE.get();

        if max_object_count == 0 {
            return;
        }

        let per_map_buffer_size = ((max_object_count - 1) * PerCameraUniformObjectData::SHADER_SIZE.get())
            + PerCameraUniform::min_size().get();

        let mut per_mat_buffer_map = ctx.data_core.graph_storage.get_mut(&self.per_material_buffer_handle);

        let new_per_mat_buffer = || {
            Arc::new(ctx.renderer.device.create_buffer(&BufferDescriptor {
                label: None,
                size: per_map_buffer_size,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }))
        };
        let buffer = match per_mat_buffer_map.entry(camera_idx) {
            Entry::Occupied(o) => {
                let r = o.into_mut();
                if r.size() != per_map_buffer_size {
                    *r = new_per_mat_buffer();
                }
                r
            }
            Entry::Vacant(o) => o.insert(new_per_mat_buffer()),
        };

        let culling = match camera_idx {
            Some(_) => wgpu::Face::Front,
            None => wgpu::Face::Back,
        };

        {
            // We don't write anything in the objects right now, as this will be filled in by the preparation compute shader
            profiling::scope!("PerCameraUniform Data Upload");
            let per_camera_data = PerCameraUniform {
                view: camera.view(),
                view_proj: camera.view_proj(),
                shadow_index: camera_idx.unwrap_or(u32::MAX as _) as u32,
                frustum: camera.world_frustum(),
                resolution: resolution.as_vec2(),
                flags: {
                    let mut flags = PerCameraUniformFlags::empty();
                    flags.set(
                        PerCameraUniformFlags::POSTIIVE_AREA_VISIBLE,
                        TriangleVisibility::from_winding_and_face(self.winding, culling).is_positive(),
                    );
                    flags.set(PerCameraUniformFlags::MULTISAMPLED, samples != SampleCount::One);
                    flags.bits()
                },
                object_count: max_object_count as u32,
                objects: Vec::new(),
            };
            let mut buffer = ctx
                .renderer
                .queue
                .write_buffer_with(buffer, 0, per_camera_data.size())
                .unwrap();
            StorageBuffer::new(&mut *buffer).write(&per_camera_data).unwrap();
        }

        let Some(object_manager_buffer) = ctx.data_core.object_manager.buffer::<M>() else {
            return;
        };
        let prep_bg = ctx.renderer.device.create_bind_group(&BindGroupDescriptor {
            label: Some(&format_sso!("UniformPrep {type_name} BG")),
            layout: &self.prep_bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: object_manager_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });

        profiling::scope!("Command Encoding");

        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} uniform bake")),
        });
        cpass.set_pipeline(&self.prep_pipeline);
        cpass.set_bind_group(0, &prep_bg, &[]);
        cpass.dispatch_workgroups(round_up_div(max_object_count as u32, WORKGROUP_SIZE), 1, 1);
        drop(cpass);
    }

    pub fn cull<M>(
        &self,
        ctx: &mut NodeExecutionContext,
        jobs: ShaderBatchDatas,
        depth_handle: DeclaredDependency<RenderTargetHandle>,
        camera_idx: Option<usize>,
    ) -> DrawCallSet
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
                camera_idx,
                CullingBuffers {
                    previous_object_reference: jobs.jobs.size().get(),
                    current_object_reference: jobs.jobs.size().get(),
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

        let per_camera_uniform = Arc::clone(
            ctx.data_core
                .graph_storage
                .get_mut(&self.per_material_buffer_handle)
                .get(&camera_idx)
                .unwrap_or_else(|| panic!("No per camera uniform for camera {:?}", camera_idx)),
        );

        {
            profiling::scope!("Culling Job Data Upload");
            let mut buffer = ctx
                .renderer
                .queue
                .write_buffer_with(&buffers.current_object_reference, 0, jobs.jobs.size())
                .unwrap();
            StorageBuffer::new(&mut *buffer).write(&jobs.jobs).unwrap();
        }

        let hi_z_buffer = ctx.graph_data.get_render_target(depth_handle);

        let culling_bg = ctx.renderer.device.create_bind_group(&BindGroupDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} BG")),
            layout: &self.culling_bgl,
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
                    resource: BindingResource::Buffer(BufferBinding {
                        buffer: &buffers.current_object_reference,
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
                BindGroupEntry {
                    binding: 9,
                    resource: per_camera_uniform.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 10,
                    resource: BindingResource::TextureView(hi_z_buffer),
                },
                BindGroupEntry {
                    binding: 11,
                    resource: BindingResource::Sampler(&self.sampler),
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

        encoder.clear_buffer(&buffers.primary_draw_call, 0, None);
        encoder.clear_buffer(&buffers.secondary_draw_call, 0, None);
        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Culling")),
        });

        cpass.set_pipeline(&self.culling_pipeline);
        for (idx, job) in jobs.jobs.iter().enumerate() {
            cpass.set_bind_group(
                0,
                &culling_bg,
                &[idx as u32 * ShaderBatchData::SHADER_SIZE.get() as u32],
            );
            cpass.dispatch_workgroups(round_up_div(job.total_invocations, WORKGROUP_SIZE), 1, 1);
        }
        drop(cpass);

        DrawCallSet {
            per_camera_uniform,
            buffers,
            draw_calls,
            material_key_ranges,
        }
    }

    pub fn add_uniform_bake_to_graph<'node, M: Material>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        camera_idx: Option<usize>,
        resolution: UVec2,
        samples: SampleCount,
        name: &str,
    ) {
        let mut node = graph.add_node(name);
        node.add_side_effect();

        node.build(move |mut ctx| {
            let camera = match camera_idx {
                Some(i) => &ctx.eval_output.shadows[i].camera,
                None => &ctx.data_core.camera_manager,
            };

            self.uniform_bake::<M>(&mut ctx, camera, camera_idx, resolution, samples);
        });
    }

    pub fn add_culling_to_graph<'node, M: Material>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        draw_calls_hdl: DataHandle<DrawCallSet>,
        depth_handle: RenderTargetHandle,
        camera_idx: Option<usize>,
        name: &str,
    ) {
        let mut node = graph.add_node(name);
        let output = node.add_data(draw_calls_hdl, NodeResourceUsage::Output);
        let depth_handle = node.add_render_target(depth_handle, NodeResourceUsage::Input);

        node.build(move |mut ctx| {
            let camera = match camera_idx {
                Some(i) => &ctx.eval_output.shadows[i].camera,
                None => &ctx.data_core.camera_manager,
            };

            let jobs = batch_objects::<M>(&mut ctx, &self.previous_invocation_map_handle, camera, camera_idx);

            if jobs.jobs.is_empty() {
                return;
            }

            let draw_calls = self.cull::<M>(&mut ctx, jobs, depth_handle, camera_idx);

            ctx.graph_data.set_data(output, Some(draw_calls));
        });
    }
}
