use crate::renderer::mesh::MeshManager;
use crate::{
    datatypes::{MeshHandle, ModelVertex},
    instruction::{InstructionStreamPair, SceneChangeInstruction},
    renderer::{options::RendererOptions, resources::RendererGlobalResources},
    RendererInitializationError,
};
use raw_window_handle::HasRawWindowHandle;
use std::{future::Future, sync::Arc};
use wgpu::{AdapterInfo, Surface, TextureFormat};

pub mod error;
pub mod limits;
mod mesh;
pub mod options;
mod resources;
mod setup;
mod util;

const SWAPCHAIN_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

pub struct Renderer {
    instructions: InstructionStreamPair,

    adapter_info: AdapterInfo,
    surface: Surface,

    global_resources: RendererGlobalResources,
    mesh_manager: MeshManager,

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

    pub fn add_mesh(&self, vertices: Vec<ModelVertex>, indices: Vec<u32>, material_count: u32) -> MeshHandle {
        let handle = self.mesh_manager.allocate();

        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::AddMesh {
                handle,
                vertices,
                indices,
                material_count,
            });

        handle
    }
}
