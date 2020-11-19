use crate::{
    datatypes::{DepthCompare, Pipeline, PipelineBindingType, PipelineHandle, PipelineInputType},
    list::RenderPassRunRate,
    registry::ResourceRegistry,
    renderer::{shaders::ShaderManager, COMPUTE_POOL, PIPELINE_BUILD_PRIORITY},
};
use parking_lot::RwLock;
use std::{future::Future, sync::Arc};
use switchyard::Switchyard;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendDescriptor,
    ColorStateDescriptor, ColorWrite, CompareFunction, CullMode, DepthStencilStateDescriptor, Device, FrontFace,
    IndexFormat, PipelineLayoutDescriptor, PolygonMode, PrimitiveTopology, ProgrammableStageDescriptor,
    RasterizationStateDescriptor, RenderPipeline, RenderPipelineDescriptor, ShaderStage, StencilStateDescriptor,
    TextureComponentType, TextureViewDimension, VertexStateDescriptor,
};

pub struct DefaultBindGroupLayouts {
    general_data: BindGroupLayout,
    object_data: BindGroupLayout,
    material_data: BindGroupLayout,
    camera_data: BindGroupLayout,
    gpu_2d_textures: Arc<BindGroupLayout>,
    gpu_cube_textures: Arc<BindGroupLayout>,
    shadow_texture: BindGroupLayout,
    skybox_texture: BindGroupLayout,
}
impl DefaultBindGroupLayouts {
    pub fn new(
        device: &Device,
        gpu_2d_textures: Arc<BindGroupLayout>,
        gpu_cube_textures: Arc<BindGroupLayout>,
    ) -> Self {
        let general_data = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("general data bgl"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                    ty: BindingType::Sampler { comparison: false },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                    ty: BindingType::Sampler { comparison: true },
                    count: None,
                },
            ],
        });

        let object_data = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("object data bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::StorageBuffer {
                    min_binding_size: None,
                    dynamic: false,
                    readonly: true,
                },
                count: None,
            }],
        });

        let material_data = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("material data bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::UniformBuffer {
                    min_binding_size: None,
                    dynamic: false,
                },
                count: None,
            }],
        });

        let camera_data = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("camera data bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::UniformBuffer {
                    min_binding_size: None,
                    dynamic: false,
                },
                count: None,
            }],
        });

        let shadow_texture = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("shadow data bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::SampledTexture {
                    dimension: TextureViewDimension::D2Array,
                    component_type: TextureComponentType::Float,
                    multisampled: false,
                },
                count: None,
            }],
        });

        let skybox_texture = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("skybox data bgl"),
            entries: &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
                ty: BindingType::SampledTexture {
                    dimension: TextureViewDimension::Cube,
                    component_type: TextureComponentType::Float,
                    multisampled: false,
                },
                count: None,
            }],
        });

        Self {
            general_data,
            object_data,
            material_data,
            camera_data,
            gpu_2d_textures,
            gpu_cube_textures,
            shadow_texture,
            skybox_texture,
        }
    }
}

pub struct CompiledPipeline {
    desc: Pipeline,
    inner: Arc<RenderPipeline>,
    uses_2d: bool,
    uses_cube: bool,
}

// TODO: invalidation based on 2d and cube manager
pub struct PipelineManager {
    default_bind_group_layouts: RwLock<DefaultBindGroupLayouts>,
    registry: RwLock<ResourceRegistry<CompiledPipeline>>,
}
impl PipelineManager {
    pub fn new(
        device: &Device,
        gpu_2d_textures: Arc<BindGroupLayout>,
        gpu_cube_textures: Arc<BindGroupLayout>,
    ) -> Arc<Self> {
        let registry = RwLock::new(ResourceRegistry::new());

        let default_bind_group_layouts =
            RwLock::new(DefaultBindGroupLayouts::new(device, gpu_2d_textures, gpu_cube_textures));

        Arc::new(Self {
            default_bind_group_layouts,
            registry,
        })
    }

    pub fn allocate_async_insert<TD>(
        self: &Arc<Self>,
        yard: &Switchyard<TD>,
        device: Arc<Device>,
        shader_manager: Arc<ShaderManager>,
        pipeline_desc: Pipeline,
    ) -> impl Future<Output = PipelineHandle>
    where
        TD: 'static,
    {
        let handle = self.registry.read().allocate();
        let this = Arc::clone(&self);
        yard.spawn(COMPUTE_POOL, PIPELINE_BUILD_PRIORITY, async move {
            let custom_layouts: Vec<_> = pipeline_desc
                .bindings
                .iter()
                .filter_map(|bind| match bind {
                    PipelineBindingType::Custom2DTexture { count } => {
                        Some(create_custom_texture_bgl(&device, TextureViewDimension::D2, *count as u32))
                    }
                    PipelineBindingType::CustomCubeTexture { count } => {
                        Some(create_custom_texture_bgl(&device, TextureViewDimension::Cube, *count as u32))
                    }
                    _ => None,
                })
                .collect();

            let mut custom_layout_iter = custom_layouts.iter();
            let mut uses_2d = false;
            let mut uses_cube = false;

            let default_bind_group_layout_guard = this.default_bind_group_layouts.read();

            let layouts: Vec<_> = pipeline_desc
                .bindings
                .iter()
                .map(|bind| match bind {
                    PipelineBindingType::GeneralData => &default_bind_group_layout_guard.general_data,
                    PipelineBindingType::ObjectData => &default_bind_group_layout_guard.object_data,
                    PipelineBindingType::Material => &default_bind_group_layout_guard.material_data,
                    PipelineBindingType::CameraData => &default_bind_group_layout_guard.camera_data,
                    PipelineBindingType::GPU2DTextures => {
                        uses_2d = true;
                        &default_bind_group_layout_guard.gpu_2d_textures
                    }
                    PipelineBindingType::GPUCubeTextures => {
                        uses_cube = true;
                        &default_bind_group_layout_guard.gpu_cube_textures
                    }
                    PipelineBindingType::ShadowTexture => &default_bind_group_layout_guard.shadow_texture,
                    PipelineBindingType::SkyboxTexture => &default_bind_group_layout_guard.skybox_texture,
                    PipelineBindingType::Custom2DTexture { .. } => custom_layout_iter.next().unwrap(),
                    PipelineBindingType::CustomCubeTexture { .. } => custom_layout_iter.next().unwrap(),
                })
                .collect();

            let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &layouts,
                push_constant_ranges: &[],
            });

            let color_states: Vec<_> = pipeline_desc
                .outputs
                .iter()
                .map(|&attachment| ColorStateDescriptor {
                    alpha_blend: BlendDescriptor::REPLACE,
                    color_blend: BlendDescriptor::REPLACE,
                    write_mask: match attachment.write {
                        true => ColorWrite::ALL,
                        false => ColorWrite::empty(),
                    },
                    format: attachment.format,
                })
                .collect();

            let depth_state = pipeline_desc.depth.map(|state| DepthStencilStateDescriptor {
                format: state.format,
                depth_write_enabled: true,
                depth_compare: match (state.compare, pipeline_desc.run_rate) {
                    // Shadow modes
                    (DepthCompare::Closer, RenderPassRunRate::PerShadow) => CompareFunction::Less,
                    (DepthCompare::CloserEqual, RenderPassRunRate::PerShadow) => CompareFunction::LessEqual,
                    (DepthCompare::Equal, RenderPassRunRate::PerShadow) => CompareFunction::Equal,
                    (DepthCompare::Further, RenderPassRunRate::PerShadow) => CompareFunction::Greater,
                    (DepthCompare::FurtherEqual, RenderPassRunRate::PerShadow) => CompareFunction::GreaterEqual,

                    // Forward modes
                    (DepthCompare::Closer, RenderPassRunRate::Once) => CompareFunction::Greater,
                    (DepthCompare::CloserEqual, RenderPassRunRate::Once) => CompareFunction::GreaterEqual,
                    (DepthCompare::Equal, RenderPassRunRate::Once) => CompareFunction::Equal,
                    (DepthCompare::Further, RenderPassRunRate::Once) => CompareFunction::Less,
                    (DepthCompare::FurtherEqual, RenderPassRunRate::Once) => CompareFunction::LessEqual,
                },
                stencil: StencilStateDescriptor::default(),
            });

            let vertex_states = [
                wgpu::VertexBufferDescriptor {
                    stride: std::mem::size_of::<crate::datatypes::ModelVertex>() as u64,
                    step_mode: wgpu::InputStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float3, 1 => Float3, 2 => Float2, 3 => Uchar4Norm, 4 => Uint],
                },
                wgpu::VertexBufferDescriptor {
                    stride: 20,
                    step_mode: wgpu::InputStepMode::Instance,
                    attributes: &[
                        wgpu::VertexAttributeDescriptor {
                            format: wgpu::VertexFormat::Uint,
                            offset: 16,
                            shader_location: 5
                        }
                    ],
                }
            ];

            let fragment_stage_module = pipeline_desc.fragment.map(|handle| shader_manager.get(handle));

            let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex_stage: ProgrammableStageDescriptor {
                    entry_point: "main",
                    module: &shader_manager.get(pipeline_desc.vertex),
                },
                fragment_stage: fragment_stage_module.as_ref().map(|module| ProgrammableStageDescriptor {
                    entry_point: "main",
                    module: &module,
                }),
                rasterization_state: Some(RasterizationStateDescriptor {
                    front_face: FrontFace::Cw,
                    cull_mode: match pipeline_desc.input {
                        PipelineInputType::FullscreenTriangle => CullMode::None,
                        PipelineInputType::Models3d => CullMode::Back,
                    },
                    polygon_mode: PolygonMode::Fill,
                    clamp_depth: match pipeline_desc.run_rate {
                        // TODO
                        RenderPassRunRate::PerShadow => false,
                        RenderPassRunRate::Once => false,
                    },
                    depth_bias: match pipeline_desc.run_rate {
                        RenderPassRunRate::PerShadow => 2,
                        RenderPassRunRate::Once => 0,
                    },
                    depth_bias_slope_scale: match pipeline_desc.run_rate {
                        RenderPassRunRate::PerShadow => 2.0,
                        RenderPassRunRate::Once => 0.0,
                    },
                    depth_bias_clamp: 0.0,
                }),
                primitive_topology: PrimitiveTopology::TriangleList,
                color_states: &color_states,
                depth_stencil_state: depth_state,
                vertex_state: VertexStateDescriptor {
                    index_format: IndexFormat::Uint32,
                    vertex_buffers: match pipeline_desc.input {
                        PipelineInputType::FullscreenTriangle => &[],
                        PipelineInputType::Models3d => &vertex_states,
                    },
                },
                sample_count: pipeline_desc.samples as u32,
                sample_mask: !0,
                alpha_to_coverage_enabled: false,
            });

            this.registry.write().insert(handle, CompiledPipeline {
                desc: pipeline_desc,
                inner: Arc::new(pipeline),
                uses_2d,
                uses_cube,
            });

            PipelineHandle(handle)
        })
    }

    pub fn get_arc(&self, handle: PipelineHandle) -> Arc<RenderPipeline> {
        Arc::clone(&self.registry.read().get(handle.0).inner)
    }

    pub fn remove(&self, handle: PipelineHandle) {
        self.registry.write().remove(handle.0);
    }
}

pub fn create_custom_texture_bgl(device: &Device, dimension: TextureViewDimension, count: u32) -> BindGroupLayout {
    let entry = BindGroupLayoutEntry {
        binding: 0,
        visibility: ShaderStage::VERTEX | ShaderStage::FRAGMENT | ShaderStage::COMPUTE,
        ty: BindingType::SampledTexture {
            dimension,
            component_type: TextureComponentType::Float,
            multisampled: false,
        },
        count: None,
    };

    let entries: Vec<_> = (0..count)
        .map(|idx| BindGroupLayoutEntry {
            binding: idx as u32,
            ..entry.clone()
        })
        .collect();

    device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: None,
        entries: &entries,
    })
}
