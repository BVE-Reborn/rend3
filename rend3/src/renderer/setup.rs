use crate::{
    graph::GraphTextureStore,
    instruction::InstructionStreamPair,
    managers::{
        CameraManager, DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, SkeletonManager,
        TextureManager,
    },
    renderer::RendererDataCore,
    util::mipmap::MipmapGenerator,
    InstanceAdapterDevice, Renderer, RendererInitializationError,
};
use parking_lot::Mutex;
use rend3_types::{Camera, Handedness, TextureFormat};
use std::sync::{atomic::AtomicUsize, Arc};
use wgpu::TextureViewDimension;

pub fn create_renderer(
    iad: InstanceAdapterDevice,
    handedness: Handedness,
    aspect_ratio: Option<f32>,
) -> Result<Arc<Renderer>, RendererInitializationError> {
    profiling::scope!("Renderer::new");

    let features = iad.device.features();
    let limits = iad.device.limits();
    let downlevel = iad.adapter.get_downlevel_properties();

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
    let material_manager = MaterialManager::new(&iad.device, iad.profile);
    let object_manager = ObjectManager::new();
    let directional_light_manager = DirectionalLightManager::new(&iad.device);
    let skeleton_manager = SkeletonManager::new();

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
    let mut profiler = wgpu_profiler::GpuProfiler::new(4, iad.queue.get_timestamp_period());
    profiler.enable_timer = features.contains(wgpu_profiler::GpuProfiler::REQUIRED_WGPU_FEATURES);

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

        current_ident: AtomicUsize::new(0),
        data_core: Mutex::new(RendererDataCore {
            camera_manager,
            mesh_manager,
            d2_texture_manager,
            d2c_texture_manager,
            material_manager,
            object_manager,
            directional_light_manager,
            skeleton_manager,
            profiler,
            graph_texture_store: GraphTextureStore::new(),
        }),

        mipmap_generator,
    }))
}
