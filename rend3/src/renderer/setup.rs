use crate::renderer::material::MaterialManager;
use crate::{
    instruction::InstructionStreamPair,
    renderer::{
        limits::{check_features, check_limits},
        mesh::MeshManager,
        options::RendererOptions,
        resources::RendererGlobalResources,
        texture::TextureManager,
        Renderer, SWAPCHAIN_FORMAT,
    },
    RendererInitializationError,
};
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;
use wgpu::{BackendBit, DeviceDescriptor, Instance, PowerPreference, RequestAdapterOptions};
use wgpu_conveyor::{AutomatedBufferManager, UploadStyle};

pub async fn create_renderer<W: HasRawWindowHandle>(
    window: &W,
    imgui: &mut imgui::Context,
    options: RendererOptions,
) -> Result<Arc<Renderer>, RendererInitializationError> {
    let instance = Instance::new(BackendBit::PRIMARY);

    let surface = unsafe { instance.create_surface(window) };

    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
        })
        .await
        .ok_or(RendererInitializationError::MissingAdapter)?;

    let adapter_info = adapter.get_info();
    let features = check_features(adapter.features())?;
    let limits = check_limits(adapter.limits())?;

    let (device, queue) = adapter
        .request_device(
            &DeviceDescriptor {
                features,
                limits,
                shader_validation: true,
            },
            None,
        )
        .await
        .map_err(|_| RendererInitializationError::RequestDeviceFailed)?;

    let mut buffer_manager = AutomatedBufferManager::new(UploadStyle::from_device_type(&adapter_info.device_type));
    let global_resources = RendererGlobalResources::new(&device, &surface, &options);
    let mesh_manager = MeshManager::new(&device);
    let texture_manager = TextureManager::new(&device);
    let material_manager = MaterialManager::new(&device, &mut buffer_manager);

    let imgui_renderer = imgui_wgpu::Renderer::new(imgui, &device, &queue, SWAPCHAIN_FORMAT);

    Ok(Arc::new(Renderer {
        instructions: InstructionStreamPair::new(),

        adapter_info,
        surface,

        buffer_manager,
        global_resources,
        mesh_manager,
        texture_manager,
        material_manager,

        imgui_renderer,

        options,
    }))
}
