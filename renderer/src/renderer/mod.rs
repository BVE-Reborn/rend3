use crate::{
    renderer::{options::RendererOptions, resources::RendererGlobalResources},
    RendererInitializationError,
};
use raw_window_handle::HasRawWindowHandle;
use std::{future::Future, sync::Arc};
use wgpu::{AdapterInfo, Surface, TextureFormat};

pub mod error;
pub mod limits;
pub mod options;
mod resources;
mod setup;
mod util;

const SWAPCHAIN_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

pub struct Renderer {
    adapter_info: AdapterInfo,
    surface: Surface,

    global_resources: RendererGlobalResources,

    imgui_renderer: imgui_wgpu::Renderer,

    options: RendererOptions,
}
impl Renderer {
    pub fn new<'a, W: HasRawWindowHandle>(
        window: &'a W,
        context: &'a mut imgui::Context,
        options: RendererOptions,
    ) -> impl Future<Output = Result<Arc<Renderer>, RendererInitializationError>> + 'a {
        setup::create_renderer(window, context, options)
    }
}
