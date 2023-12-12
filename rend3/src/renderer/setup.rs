use std::sync::Arc;

use parking_lot::Mutex;
use rend3_types::{Camera, Handedness, TextureFormat};
use wgpu::TextureViewDimension;
use wgpu_profiler::GpuProfilerSettings;

use crate::{
    graph::GraphTextureStore,
    instruction::InstructionStreamPair,
    managers::{
        CameraManager, DirectionalLightManager, GraphStorage, MaterialManager, MeshManager, ObjectManager,
        PointLightManager, SkeletonManager, TextureManager,
    },
    renderer::{HandleAllocators, RendererDataCore},
    util::{mipmap::MipmapGenerator, scatter_copy::ScatterCopy},
    InstanceAdapterDevice, Renderer, RendererInitializationError,
};

pub fn create_renderer(
    iad: InstanceAdapterDevice,
    handedness: Handedness,
    aspect_ratio: Option<f32>,
) -> Result<Arc<Renderer>, RendererInitializationError> {
    profiling::scope!("Renderer::new");

    let features = iad.device.features();
    let limits = iad.device.limits();
    let downlevel = iad.adapter.get_downlevel_capabilities();

    let camera_manager = CameraManager::new(Camera::default(), handedness, aspect_ratio);

    let d2_texture_manager = TextureManager::new(
        &iad.device,
        iad.profile,
        limits.max_sampled_textures_per_shader_stage,
        TextureViewDimension::D2,
    );
    let d2c_texture_manager = TextureManager::new(
        &iad.device,
        iad.profile,
        limits.max_sampled_textures_per_shader_stage,
        TextureViewDimension::Cube,
    );
    let mesh_manager = MeshManager::new(&iad.device);
    let material_manager = MaterialManager::new(&iad.device);
    let object_manager = ObjectManager::new();
    let directional_light_manager = DirectionalLightManager::new(&iad.device);
    let point_light_manager = PointLightManager::new(&iad.device);
    let skeleton_manager = SkeletonManager::new();
    let graph_storage = GraphStorage::new();

    let mipmap_generator = MipmapGenerator::new(
        &iad.device,
        &[
            TextureFormat::Rgba8Unorm,
            TextureFormat::Rgba8UnormSrgb,
            TextureFormat::Bgra8Unorm,
            TextureFormat::Bgra8UnormSrgb,
            TextureFormat::Rgba16Float,
        ],
    );

    let profiler = Mutex::new(
        wgpu_profiler::GpuProfiler::new(GpuProfilerSettings {
            enable_timer_scopes: true,
            enable_debug_groups: true,
            max_num_pending_frames: 4,
        })
        .map_err(RendererInitializationError::GpuProfilerCreation)?,
    );

    let scatter = ScatterCopy::new(&iad.device);

    Ok(Arc::new(Renderer {
        instructions: InstructionStreamPair::new(),

        profile: iad.profile,
        adapter_info: iad.info,
        queue: iad.queue,
        device: iad.device,

        features,
        limits,
        downlevel,
        handedness,

        resource_handle_allocators: HandleAllocators::default(),
        mesh_manager,
        data_core: Mutex::new(RendererDataCore {
            camera_manager,
            d2_texture_manager,
            d2c_texture_manager,
            material_manager,
            object_manager,
            directional_light_manager,
            point_light_manager,
            skeleton_manager,
            graph_storage,
            profiler,
            graph_texture_store: GraphTextureStore::new(),
        }),

        mipmap_generator,
        scatter,
    }))
}
