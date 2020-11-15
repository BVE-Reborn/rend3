use crate::datatypes::{Pipeline, PipelineBindingType, PipelineHandle};
use crate::registry::ResourceRegistry;
use crate::renderer::{COMPUTE_POOL, PIPELINE_BUILD_PRIORITY};
use parking_lot::RwLock;
use std::future::Future;
use std::sync::Arc;
use switchyard::Switchyard;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType, Device, PipelineLayoutDescriptor,
    ProgrammableStageDescriptor, RenderPipeline, RenderPipelineDescriptor, ShaderStage, TextureComponentType,
    TextureViewDimension,
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
                ty: BindingType::StorageBuffer {
                    min_binding_size: None,
                    dynamic: false,
                    readonly: true,
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
    uses_3d: bool,
}

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
        yard: Switchyard<TD>,
        device: Arc<Device>,
        pipeline: Pipeline,
    ) -> impl Future<Output = PipelineHandle>
    where
        TD: 'static,
    {
        let id = self.registry.read().allocate();
        let this = Arc::clone(&self);
        yard.spawn(COMPUTE_POOL, PIPELINE_BUILD_PRIORITY, async move {
            let custom_layouts: Vec<_> = pipeline
                .bindings
                .iter()
                .filter_map(|bind| match bind {
                    PipelineBindingType::Custom2DTexture { count } => {
                        Some(create_custom_texture_bgl(&device, TextureViewDimension::D2))
                    }
                    PipelineBindingType::CustomCubeTexture { count } => {
                        Some(create_custom_texture_bgl(&device, TextureViewDimension::Cube))
                    }
                    _ => None,
                })
                .collect();

            let mut custom_layout_iter = custom_layouts.iter();
            let mut used_2d = false;
            let mut used_cube = false;

            let default_bind_group_layout_guard = this.default_bind_group_layouts.read();

            let layouts: Vec<_> = pipeline
                .bindings
                .iter()
                .map(|bind| match bind {
                    PipelineBindingType::GeneralData => &default_bind_group_layout_guard.general_data,
                    PipelineBindingType::ObjectData => &default_bind_group_layout_guard.object_data,
                    PipelineBindingType::Material => &default_bind_group_layout_guard.material_data,
                    PipelineBindingType::CameraData => &default_bind_group_layout_guard.camera_data,
                    PipelineBindingType::GPU2DTextures => {
                        used_2d = true;
                        &default_bind_group_layout_guard.gpu_2d_textures
                    }
                    PipelineBindingType::GPUCubeTextures => {
                        used_cube = true;
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

            device.create_render_pipeline(&RenderPipelineDescriptor {
                label: None,
                layout: Some(&pipeline_layout),
                vertex_stage: ProgrammableStageDescriptor {},
                fragment_stage: None,
                rasterization_state: None,
                primitive_topology: PrimitiveTopology::PointList,
                color_states: &[],
                depth_stencil_state: None,
                vertex_state: VertexStateDescriptor {},
                sample_count: 0,
                sample_mask: 0,
                alpha_to_coverage_enabled: false,
            })
        })
    }
}

fn create_custom_texture_bgl(device: &Device, dimension: TextureViewDimension) -> BindGroupLayout {
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

    let entries: Vec<_> = (0..(*count))
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
