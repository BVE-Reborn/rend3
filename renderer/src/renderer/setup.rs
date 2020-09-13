use crate::{
    renderer::{
        limits::{check_features, check_limits},
        options::RendererOptions,
        resources::RendererGlobalResources,
        Renderer, SWAPCHAIN_FORMAT,
    },
    RendererInitializationError,
};
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;
use wgpu::{BackendBit, DeviceDescriptor, Instance, PowerPreference, RequestAdapterOptions};

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

    let global_resources = RendererGlobalResources::new(&device, &surface, &options);

    let imgui_renderer = imgui_wgpu::Renderer::new(imgui, &device, &queue, SWAPCHAIN_FORMAT);

    Ok(Arc::new(Renderer {
        adapter_info,
        surface,

        global_resources,

        imgui_renderer,

        options,
    }))
}
