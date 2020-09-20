use crate::{
    datatypes::{
        AffineTransform, Material, MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
    },
    instruction::{Instruction, InstructionStreamPair},
    renderer::{
        material::MaterialManager, mesh::MeshManager, object::ObjectManager, options::RendererOptions,
        resources::RendererGlobalResources, shaders::ShaderManager, texture::TextureManager,
    },
    statistics::RendererStatistics,
    RendererInitializationError, TLS,
};
use parking_lot::RwLock;
use raw_window_handle::HasRawWindowHandle;
use std::{cell::RefCell, future::Future, sync::Arc};
use switchyard::{JoinHandle, Switchyard};
use wgpu::{AdapterInfo, Device, Queue, Surface, TextureFormat};
use wgpu_conveyor::AutomatedBufferManager;

pub mod error;
pub mod limits;
mod material;
mod mesh;
mod object;
pub mod options;
mod render;
mod resources;
mod setup;
mod shaders;
mod texture;
mod util;

const COMPUTE_POOL: u8 = 0;

const SHADER_COMPILE_PRIORITY: u32 = 0;
const MAIN_TASK_PRIORITY: u32 = 1;

const SWAPCHAIN_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

pub struct Renderer<TLD = TLS>
where
    TLD: AsMut<TLS> + 'static,
{
    yard: Arc<Switchyard<RefCell<TLD>>>,
    instructions: InstructionStreamPair,

    adapter_info: AdapterInfo,
    queue: Queue,
    device: Device,
    surface: Surface,

    buffer_manager: AutomatedBufferManager,
    global_resources: RwLock<RendererGlobalResources>,
    shader_manager: ShaderManager,
    mesh_manager: MeshManager,
    texture_manager: TextureManager,
    material_manager: MaterialManager,
    object_manager: ObjectManager,

    imgui_renderer: imgui_wgpu::Renderer,

    options: RendererOptions,
}
impl<TLD> Renderer<TLD>
where
    TLD: AsMut<TLS> + 'static,
{
    pub fn new<'a, W: HasRawWindowHandle>(
        window: &'a W,
        yard: Arc<Switchyard<RefCell<TLD>>>,
        context: &'a mut imgui::Context,
        options: RendererOptions,
    ) -> impl Future<Output = Result<Arc<Self>, RendererInitializationError>> + 'a {
        setup::create_renderer(window, yard, context, options)
    }

    pub fn add_mesh(&self, mesh: Mesh) -> MeshHandle {
        let handle = self.mesh_manager.allocate();

        self.instructions
            .producer
            .lock()
            .push(Instruction::AddMesh { handle, mesh });

        handle
    }

    pub fn remove_mesh(&self, handle: MeshHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveMesh { mesh: handle });
    }

    pub fn add_texture(&self, texture: Texture) -> TextureHandle {
        let handle = self.texture_manager.allocate();
        self.instructions
            .producer
            .lock()
            .push(Instruction::AddTexture { handle, texture });
        handle
    }

    pub fn remove_texture(&self, handle: TextureHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveTexture { texture: handle })
    }

    pub fn add_material(&self, material: Material) -> MaterialHandle {
        let handle = self.material_manager.allocate();
        self.instructions
            .producer
            .lock()
            .push(Instruction::AddMaterial { handle, material });
        handle
    }

    pub fn remove_material(&self, handle: MaterialHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveMaterial { material: handle });
    }

    pub fn add_object(&self, object: Object) -> ObjectHandle {
        let handle = self.object_manager.allocate();
        self.instructions
            .producer
            .lock()
            .push(Instruction::AddObject { handle, object });
        handle
    }

    pub fn set_object_transform(&self, handle: ObjectHandle, transform: AffineTransform) {
        self.instructions.producer.lock().push(Instruction::SetObjectTransform {
            object: handle,
            transform,
        });
    }

    pub fn remove_object(&self, handle: ObjectHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveObject { object: handle })
    }

    pub fn set_options(&self, options: RendererOptions) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetOptions { options })
    }

    pub fn render(self: &Arc<Self>) -> JoinHandle<RendererStatistics> {
        self.yard
            .spawn(0, MAIN_TASK_PRIORITY, render::render_loop(Arc::clone(self)))
    }
}
