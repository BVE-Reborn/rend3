use std::{
    any::{type_name, TypeId},
    borrow::Cow,
    collections::{hash_map::Entry, HashMap},
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
    types::{GraphDataHandle, Material, MaterialArray, SampleCount, VERTEX_ATTRIBUTE_POSITION},
    util::{frustum::Frustum, math::IntegerExt, typedefs::FastHashMap},
    Renderer, ShaderPreProcessor, ShaderVertexBufferConfig,
};
use wgpu::{
    self, AddressMode, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, Buffer, BufferBinding, BufferBindingType, BufferDescriptor,
    BufferUsages, CommandEncoder, ComputePassDescriptor, ComputePipeline, ComputePipelineDescriptor, Device,
    FilterMode, PipelineLayoutDescriptor, Queue, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderModuleDescriptor, ShaderStages, TextureSampleType, TextureViewDimension,
};

use crate::{
    common::CameraIndex,
    culling::{
        batching::{batch_objects, JobSubRegion, PerCameraPreviousInvocationsMap, ShaderBatchData, ShaderBatchDatas},
        suballoc::InputOutputBuffer,
        WORKGROUP_SIZE,
    },
};

#[derive(Debug)]
pub struct DrawCallSet {
    pub culling_data_buffer: Buffer,
    pub per_camera_uniform: Arc<Buffer>,
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
pub struct CullingBufferMap {
    inner: FastHashMap<CameraIndex, CullingBuffers>,
}
impl CullingBufferMap {
    pub fn get_buffers(&self, camera: CameraIndex) -> Option<&CullingBuffers> {
        self.inner.get(&camera)
    }

    fn get_or_resize_buffers(
        &mut self,
        queue: &Queue,
        device: &Device,
        encoder: &mut CommandEncoder,
        camera: CameraIndex,
        sizes: CullingBufferSizes,
    ) -> &mut CullingBuffers {
        match self.inner.entry(camera) {
            Entry::Occupied(b) => {
                let b = b.into_mut();

                b.update_sizes(queue, device, encoder, sizes);

                b
            }
            Entry::Vacant(b) => b.insert(CullingBuffers::new(device, queue, sizes)),
        }
    }
}

struct CullingBufferSizes {
    invocations: u64,
    draw_calls: u64,
}

#[derive(Debug)]
pub struct CullingBuffers {
    pub index_buffer: InputOutputBuffer,
    pub draw_call_buffer: InputOutputBuffer,
    pub culling_results_buffer: InputOutputBuffer,
}

impl CullingBuffers {
    fn new(device: &Device, queue: &Queue, sizes: CullingBufferSizes) -> Self {
        Self {
            // One element per triangle/invocation
            index_buffer: InputOutputBuffer::new(device, queue, sizes.invocations, "Index Buffer", 4, 4, false),
            draw_call_buffer: InputOutputBuffer::new(device, queue, sizes.draw_calls, "Draw Call Buffer", 20, 4, true),
            culling_results_buffer: InputOutputBuffer::new(
                device,
                queue,
                // 32 bits in a u32
                sizes.invocations.div_round_up(u32::BITS as _),
                "Culling Results Buffer",
                4,
                4,
                false,
            ),
        }
    }

    fn update_sizes(
        &mut self,
        queue: &Queue,
        device: &Device,
        encoder: &mut CommandEncoder,
        sizes: CullingBufferSizes,
    ) {
        self.index_buffer.swap(queue, device, encoder, sizes.invocations * 3);
        self.draw_call_buffer.swap(queue, device, encoder, sizes.draw_calls);
        self.culling_results_buffer
            .swap(queue, device, encoder, sizes.invocations.div_round_up(32));
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
    per_material_buffer_handle: GraphDataHandle<HashMap<CameraIndex, Arc<Buffer>>>,
    pub culling_buffer_map_handle: GraphDataHandle<CullingBufferMap>,
    previous_invocation_map_handle: GraphDataHandle<PerCameraPreviousInvocationsMap>,
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
                &serde_json::json! {{
                    "position_attribute_offset": position_offset,
                }},
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
                // Vertex Buffer
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
                // Object Buffer
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
                // Draw Calls
                BindGroupLayoutEntry {
                    binding: 3,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(20 + 8).unwrap()),
                    },
                    count: None,
                },
                // Index buffer
                BindGroupLayoutEntry {
                    binding: 4,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(4 + 8).unwrap()),
                    },
                    count: None,
                },
                // Culling Results
                BindGroupLayoutEntry {
                    binding: 5,
                    visibility: ShaderStages::COMPUTE,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: Some(NonZeroU64::new(4 + 8).unwrap()),
                    },
                    count: None,
                },
                // per camera uniforms
                BindGroupLayoutEntry {
                    binding: 6,
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
                    binding: 7,
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
                    binding: 8,
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
        let previous_invocation_map_handle = renderer.add_graph_data(PerCameraPreviousInvocationsMap::new());

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

    pub fn object_uniform_upload<M>(
        &self,
        ctx: &mut NodeExecutionContext,
        camera: &CameraManager,
        camera_idx: CameraIndex,
        resolution: UVec2,
        samples: SampleCount,
    ) where
        M: Material,
    {
        profiling::scope!("GpuCuller::object_uniform_upload");

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
            CameraIndex::Shadow(_) => wgpu::Face::Front,
            CameraIndex::Viewport => wgpu::Face::Back,
        };

        {
            // We don't write anything in the objects right now, as this will be filled in by the preparation compute shader
            profiling::scope!("PerCameraUniform Data Upload");
            let per_camera_data = PerCameraUniform {
                view: camera.view(),
                view_proj: camera.view_proj(),
                shadow_index: camera_idx.to_shader_index(),
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
            timestamp_writes: None,
        });
        cpass.set_pipeline(&self.prep_pipeline);
        cpass.set_bind_group(0, &prep_bg, &[]);
        cpass.dispatch_workgroups((max_object_count as u32).div_round_up(WORKGROUP_SIZE), 1, 1);
        drop(cpass);
    }

    pub fn cull<M>(
        &self,
        ctx: &mut NodeExecutionContext,
        jobs: ShaderBatchDatas,
        depth_handle: DeclaredDependency<RenderTargetHandle>,
        camera_idx: CameraIndex,
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
                debug_assert_eq!(j.total_invocations % WORKGROUP_SIZE, 0);
                j.total_invocations
            })
            .sum();

        let encoder = ctx.encoder_or_pass.take_encoder();

        let mut culling_buffer_map = ctx.data_core.graph_storage.get_mut(&self.culling_buffer_map_handle);
        let buffers = culling_buffer_map.get_or_resize_buffers(
            &ctx.renderer.queue,
            &ctx.renderer.device,
            encoder,
            camera_idx,
            CullingBufferSizes {
                invocations: total_invocations as u64,
                draw_calls: jobs.regions.len() as u64,
            },
        );

        let per_camera_uniform = Arc::clone(
            ctx.data_core
                .graph_storage
                .get_mut(&self.per_material_buffer_handle)
                .get(&camera_idx)
                .unwrap_or_else(|| panic!("No per camera uniform for camera {:?}", camera_idx)),
        );

        let culling_data_buffer = {
            profiling::scope!("Culling Job Data Upload");

            let culling_data_buffer = ctx.renderer.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Culling Data Buffer"),
                size: jobs.jobs.size().get(),
                usage: wgpu::BufferUsages::STORAGE,
                mapped_at_creation: true,
            });

            let mut mapping = culling_data_buffer.slice(..).get_mapped_range_mut();
            StorageBuffer::new(&mut *mapping).write(&jobs.jobs).unwrap();
            drop(mapping);
            culling_data_buffer.unmap();

            culling_data_buffer
        };

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
                        buffer: &culling_data_buffer,
                        offset: 0,
                        size: Some(ShaderBatchData::SHADER_SIZE),
                    }),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: buffers.draw_call_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 4,
                    resource: buffers.index_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 5,
                    resource: buffers.culling_results_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 6,
                    resource: per_camera_uniform.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 7,
                    resource: BindingResource::TextureView(hi_z_buffer),
                },
                BindGroupEntry {
                    binding: 8,
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

        encoder.clear_buffer(&buffers.draw_call_buffer, 8, None);
        let mut cpass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some(&format_sso!("GpuCuller {type_name} Culling")),
            timestamp_writes: None,
        });

        cpass.set_pipeline(&self.culling_pipeline);
        for (idx, job) in jobs.jobs.iter().enumerate() {
            // RA can't infer this
            let job: &ShaderBatchData = job;

            cpass.set_bind_group(
                0,
                &culling_bg,
                &[idx as u32 * ShaderBatchData::SHADER_SIZE.get() as u32],
            );
            cpass.dispatch_workgroups(job.total_invocations.div_round_up(WORKGROUP_SIZE), 1, 1);
        }
        drop(cpass);

        DrawCallSet {
            culling_data_buffer,
            per_camera_uniform,
            draw_calls,
            material_key_ranges,
        }
    }

    pub fn add_object_uniform_upload_to_graph<'node, M: Material>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        camera_idx: CameraIndex,
        resolution: UVec2,
        samples: SampleCount,
        name: &str,
    ) {
        let mut node = graph.add_node(name);
        node.add_side_effect();

        node.build(move |mut ctx| {
            let camera = match camera_idx {
                CameraIndex::Shadow(i) => &ctx.eval_output.shadows[i as usize].camera,
                CameraIndex::Viewport => &ctx.data_core.camera_manager,
            };

            self.object_uniform_upload::<M>(&mut ctx, camera, camera_idx, resolution, samples);
        });
    }

    pub fn add_culling_to_graph<'node, M: Material>(
        &'node self,
        graph: &mut RenderGraph<'node>,
        draw_calls_hdl: DataHandle<Arc<DrawCallSet>>,
        depth_handle: RenderTargetHandle,
        camera_idx: CameraIndex,
        name: &str,
    ) {
        let mut node = graph.add_node(name);
        let output = node.add_data(draw_calls_hdl, NodeResourceUsage::Output);
        let depth_handle = node.add_render_target(depth_handle, NodeResourceUsage::Input);

        node.build(move |mut ctx| {
            let camera = match camera_idx {
                CameraIndex::Shadow(i) => &ctx.eval_output.shadows[i as usize].camera,
                CameraIndex::Viewport => &ctx.data_core.camera_manager,
            };

            let jobs = batch_objects::<M>(&mut ctx, &self.previous_invocation_map_handle, camera, camera_idx);

            if jobs.jobs.is_empty() {
                return;
            }

            let draw_calls = self.cull::<M>(&mut ctx, jobs, depth_handle, camera_idx);

            ctx.graph_data.set_data(output, Some(Arc::new(draw_calls)));
        });
    }
}
