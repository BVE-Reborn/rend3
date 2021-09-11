use crate::{
    instruction::InstructionStreamPair,
    resources::{
        CameraManager, DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, TextureManager,
        STARTING_2D_TEXTURES, STARTING_CUBE_TEXTURES,
    },
    util::mipmap::MipmapGenerator,
    InstanceAdapterDevice, Renderer, RendererInitializationError,
};
use parking_lot::{Mutex, RwLock};
use rend3_types::{Camera, TextureFormat};
use std::sync::Arc;
use wgpu::TextureViewDimension;

pub fn create_renderer(
    iad: InstanceAdapterDevice,
    aspect_ratio: Option<f32>,
) -> Result<Arc<Renderer>, RendererInitializationError> {
    let features = iad.device.features();

    let camera_manager = RwLock::new(CameraManager::new(Camera::default(), aspect_ratio));

    let texture_manager_2d = RwLock::new(TextureManager::new(
        &iad.device,
        iad.mode,
        STARTING_2D_TEXTURES,
        TextureViewDimension::D2,
    ));
    let texture_manager_cube = RwLock::new(TextureManager::new(
        &iad.device,
        iad.mode,
        STARTING_CUBE_TEXTURES,
        TextureViewDimension::Cube,
    ));
    let mesh_manager = RwLock::new(MeshManager::new(&iad.device));
    let material_manager = RwLock::new(MaterialManager::new(&iad.device, iad.mode));
    let object_manager = RwLock::new(ObjectManager::new());
    let directional_light_manager = RwLock::new(DirectionalLightManager::new(&iad.device));

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

        mode: iad.mode,
        adapter_info: iad.info,
        instance: iad.instance,
        queue: iad.queue,
        device: iad.device,

        camera_manager,
        mesh_manager,
        d2_texture_manager: texture_manager_2d,
        d2c_texture_manager: texture_manager_cube,
        material_manager,
        object_manager,
        directional_light_manager,

        mipmap_generator: Mutex::new(mipmap_generator),
        profiler: Mutex::new(profiler),
    }))
}
