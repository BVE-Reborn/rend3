use crate::{
    datatypes::{
        AffineTransform, Material, MaterialHandle, Mesh, MeshHandle, ModelVertex, Object, ObjectHandle,
        RendererTextureFormat, Texture, TextureHandle,
    },
    instruction::{InstructionStreamPair, SceneChangeInstruction},
    renderer::{
        material::MaterialManager, mesh::MeshManager, object::ObjectManager, options::RendererOptions,
        resources::RendererGlobalResources, texture::TextureManager,
    },
    RendererInitializationError,
};
use raw_window_handle::HasRawWindowHandle;
use std::{future::Future, sync::Arc};
use wgpu::{AdapterInfo, Surface, TextureFormat};
use wgpu_conveyor::AutomatedBufferManager;

pub mod error;
pub mod limits;
mod material;
mod mesh;
mod object;
pub mod options;
mod resources;
mod setup;
mod texture;
mod util;

const SWAPCHAIN_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

pub struct Renderer {
    instructions: InstructionStreamPair,

    adapter_info: AdapterInfo,
    surface: Surface,

    buffer_manager: AutomatedBufferManager,
    global_resources: RendererGlobalResources,
    mesh_manager: MeshManager,
    texture_manager: TextureManager,
    material_manager: MaterialManager,
    object_manager: ObjectManager,

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

    pub fn add_mesh(&self, mesh: Mesh) -> MeshHandle {
        let handle = self.mesh_manager.allocate();

        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::AddMesh { handle, mesh });

        handle
    }

    pub fn remove_mesh(&self, handle: MeshHandle) {
        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::RemoveMesh { mesh: handle });
    }

    pub fn add_texture(&self, texture: Texture) -> TextureHandle {
        let handle = self.texture_manager.allocate();
        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::AddTexture { handle, texture });
        handle
    }

    pub fn remove_texture(&self, handle: TextureHandle) {
        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::RemoveTexture { texture: handle })
    }

    pub fn add_material(&self, material: Material) -> MaterialHandle {
        let handle = self.material_manager.allocate();
        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::AddMaterial { handle, material });
        handle
    }

    pub fn remove_material(&self, handle: MaterialHandle) {
        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::RemoveMaterial { material: handle });
    }

    pub fn add_object(&self, object: Object) -> ObjectHandle {
        let handle = self.object_manager.allocate();
        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::AddObject { handle, object });
        handle
    }

    pub fn set_object_transform(&self, handle: ObjectHandle, transform: AffineTransform) {
        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::SetObjectTransform {
                object: handle,
                transform,
            });
    }

    pub fn remove_object(&self, handle: ObjectHandle) {
        self.instructions
            .producer
            .scene_change
            .write()
            .push(SceneChangeInstruction::RemoveObject { object: handle })
    }
}
