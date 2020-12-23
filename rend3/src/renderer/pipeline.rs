use crate::{
    datatypes::{DepthCompare, Pipeline, PipelineBindingType, PipelineHandle, PipelineInputType},
    list::RenderPassRunRate,
    registry::ResourceRegistry,
    Renderer, RendererMode,
};
use parking_lot::RwLock;
use std::{future::Future, sync::Arc};
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, BlendDescriptor,
    ColorStateDescriptor, ColorWrite, CompareFunction, CullMode, DepthStencilStateDescriptor, Device, FrontFace,
    IndexFormat, PipelineLayoutDescriptor, PrimitiveTopology, ProgrammableStageDescriptor, PushConstantRange,
    RasterizationStateDescriptor, RenderPipeline, RenderPipelineDescriptor, ShaderStage, StencilStateDescriptor,
    TextureComponentType, TextureViewDimension, VertexStateDescriptor,
};

#[derive(Debug)]
pub struct CompiledPipeline {
    desc: Pipeline,
    inner: Arc<RenderPipeline>,
    uses_2d: bool,
    uses_cube: bool,
}

// TODO: invalidation based on 2d and cube manager
pub struct PipelineManager {
    registry: RwLock<ResourceRegistry<CompiledPipeline>>,
}
impl PipelineManager {
    pub fn new() -> Arc<Self> {
        let registry = RwLock::new(ResourceRegistry::new());

        Arc::new(Self { registry })
    }

    pub fn allocate_async_insert<TD>(
        self: &Arc<Self>,
        renderer: Arc<Renderer<TD>>,
        pipeline_desc: Pipeline,
    ) -> impl Future<Output = PipelineHandle>
    where
        TD: 'static,
    {
        let handle = self.registry.read().allocate();
        let this = Arc::clone(&self);
        let renderer_clone = Arc::clone(&renderer);
        renderer_clone.yard.spawn(renderer.yard_priorites.compute_pool, renderer.yard_priorites.pipeline_build_priority, async move {
            let custom_layouts: Vec<_> = pipeline_desc
                .bindings
                .iter()
                .filter_map(|bind| match bind {
                    PipelineBindingType::Custom2DTexture { count } => {
                        Some(create_custom_texture_bgl(&renderer.device, TextureViewDimension::D2, *count as u32))
                    }
                    PipelineBindingType::CustomCubeTexture { count } => {
                        Some(create_custom_texture_bgl(&renderer.device, TextureViewDimension::Cube, *count as u32))
                    }
                    _ => None,
                })
                .collect();

            let mut custom_layout_iter = custom_layouts.iter();
            let mut uses_2d = false;
            let mut uses_cube = false;

            let global_data = renderer.global_resources.read();
            let texture_2d = renderer.texture_manager_2d.read();
            let texture_cube = renderer.texture_manager_cube.read();

            let layouts: Vec<_> = pipeline_desc
                .bindings
                .iter()
                .map(|bind| match bind {
                    PipelineBindingType::GeneralData => &global_data.general_bgl,
                    PipelineBindingType::ObjectData => &global_data.object_data_bgl,
                    PipelineBindingType::CPUMaterial | PipelineBindingType::GPUMaterial => &global_data.material_bgl,
                    PipelineBindingType::CameraData => &global_data.camera_data_bgl,
                    PipelineBindingType::GPU2DTextures => {
                        uses_2d = true;
                        texture_2d.gpu_bind_group_layout()
                    }
                    PipelineBindingType::GPUCubeTextures => {
                        uses_cube = true;
                        texture_cube.gpu_bind_group_layout()
                    }
                    PipelineBindingType::ShadowTexture => &global_data.shadow_texture_bgl,
                    PipelineBindingType::SkyboxTexture => &global_data.skybox_bgl,
                    PipelineBindingType::Custom2DTexture { .. } => custom_layout_iter.next().unwrap(),
                    PipelineBindingType::CustomCubeTexture { .. } => custom_layout_iter.next().unwrap(),
                })
                .collect();

            let cpu_push_constants = [PushConstantRange {
                range: 0..4,
                stages: ShaderStage::VERTEX | ShaderStage::FRAGMENT
            }];

            let push_constant_ranges = match renderer.mode {
                RendererMode::CPUPowered => {
                    &cpu_push_constants[..]
                }
                RendererMode::GPUPowered => {
                    &[]
                }
            };

            let pipeline_layout = renderer.device.create_pipeline_layout(&PipelineLayoutDescriptor {
                label: None,
                bind_group_layouts: &layouts,
                push_constant_ranges,
            });

            drop((global_data, texture_2d, texture_cube));

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

            let fragment_stage_module = pipeline_desc.fragment.map(|handle| renderer.shader_manager.get(handle));

            let pipeline = renderer.device.create_render_pipeline(&RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex_stage: ProgrammableStageDescriptor {
                    entry_point: "main",
                    module: &renderer.shader_manager.get(pipeline_desc.vertex),
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
                        PipelineInputType::Models3d => match renderer.mode {
                            RendererMode::CPUPowered => {
                                &vertex_states[0..1]
                            }
                            RendererMode::GPUPowered => {
                                &vertex_states
                            }
                        },
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
