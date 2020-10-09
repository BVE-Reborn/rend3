use crate::{
    datatypes::{
        AffineTransform, CameraLocation, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material,
        MaterialChange, MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
    },
    instruction::{Instruction, InstructionStreamPair},
    renderer::{
        info::ExtendedAdapterInfo, material::MaterialManager, mesh::MeshManager, object::ObjectManager,
        passes::ForwardPassSet, resources::RendererGlobalResources, shaders::ShaderManager, texture::TextureManager,
    },
    statistics::RendererStatistics,
    RendererInitializationError, RendererOptions,
};
use parking_lot::{Mutex, RwLock};
use raw_window_handle::HasRawWindowHandle;
use std::{future::Future, sync::Arc};
use switchyard::{JoinHandle, Switchyard};
use wgpu::{Device, Queue, Surface, TextureFormat};
use wgpu_conveyor::AutomatedBufferManager;

#[macro_use]
mod util;

mod camera;
pub mod error;
mod frustum;
mod info;
mod light {
    pub mod directional;

    pub use directional::*;
}
pub mod limits;
mod material;
mod mesh;
mod object;
mod passes {
    mod blit;
    mod culling;
    mod depth;
    mod forward_set;
    mod opaque;
    mod shadow_set;
    mod skybox;

    pub use blit::*;
    pub use culling::*;
    pub use depth::*;
    pub use forward_set::*;
    pub use opaque::*;
    pub use shadow_set::*;
    pub use skybox::*;
}
mod render;
mod resources;
mod setup;
mod shaders;
mod texture;
mod uniforms;

const COMPUTE_POOL: u8 = 0;

const BUFFER_RECALL_PRIORITY: u32 = 0;
const MAIN_TASK_PRIORITY: u32 = 1;

const INTERNAL_RENDERBUFFER_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
const INTERNAL_RENDERBUFFER_NORMAL_FORMAT: TextureFormat = TextureFormat::Rgba16Float;
const INTERNAL_RENDERBUFFER_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;
const INTERNAL_SHADOW_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth24Plus;
const SWAPCHAIN_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

pub struct Renderer<TLD = ()>
where
    TLD: 'static,
{
    yard: Arc<Switchyard<TLD>>,
    instructions: InstructionStreamPair,

    _adapter_info: ExtendedAdapterInfo,
    queue: Queue,
    device: Arc<Device>,
    surface: Surface,

    buffer_manager: Mutex<AutomatedBufferManager>,
    global_resources: RwLock<RendererGlobalResources>,
    _shader_manager: ShaderManager,
    mesh_manager: RwLock<MeshManager>,
    texture_manager_2d: RwLock<TextureManager>,
    texture_manager_cube: RwLock<TextureManager>,
    texture_manager_internal: RwLock<TextureManager>,
    material_manager: RwLock<MaterialManager>,
    object_manager: RwLock<ObjectManager>,
    directional_light_manager: RwLock<light::DirectionalLightManager>,

    forward_pass_set: ForwardPassSet,

    swapchain_blit_pass: passes::BlitPass,
    culling_pass: passes::CullingPass,
    skybox_pass: RwLock<passes::SkyboxPass>,
    depth_pass: RwLock<passes::DepthPass>,
    opaque_pass: RwLock<passes::OpaquePass>,

    _imgui_renderer: imgui_wgpu::Renderer,

    options: RwLock<RendererOptions>,
}
impl<TLD: 'static> Renderer<TLD> {
    pub fn new<'a, W: HasRawWindowHandle>(
        window: &'a W,
        yard: Arc<Switchyard<TLD>>,
        imgui_context: &'a mut imgui::Context,
        options: RendererOptions,
    ) -> impl Future<Output = Result<Arc<Self>, RendererInitializationError>> + 'a {
        setup::create_renderer(window, yard, imgui_context, options)
    }

    pub fn add_mesh(&self, mesh: Mesh) -> MeshHandle {
        let handle = self.mesh_manager.read().allocate();

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
            .push(Instruction::RemoveMesh { handle });
    }

    pub fn add_texture_2d(&self, texture: Texture) -> TextureHandle {
        let handle = self.texture_manager_2d.read().allocate();
        self.instructions
            .producer
            .lock()
            .push(Instruction::AddTexture2D { handle, texture });
        handle
    }

    pub fn remove_texture_2d(&self, handle: TextureHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveTexture2D { handle })
    }

    pub fn add_texture_cube(&self, texture: Texture) -> TextureHandle {
        let handle = self.texture_manager_cube.read().allocate();
        self.instructions
            .producer
            .lock()
            .push(Instruction::AddTextureCube { handle, texture });
        handle
    }

    pub fn remove_texture_dube(&self, handle: TextureHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveTextureCube { handle })
    }

    pub fn add_material(&self, material: Material) -> MaterialHandle {
        let handle = self.material_manager.read().allocate();
        self.instructions
            .producer
            .lock()
            .push(Instruction::AddMaterial { handle, material });
        handle
    }

    pub fn update_material(&self, handle: MaterialHandle, change: MaterialChange) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::ChangeMaterial { handle, change })
    }

    pub fn remove_material(&self, handle: MaterialHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveMaterial { handle });
    }

    pub fn add_object(&self, object: Object) -> ObjectHandle {
        let handle = self.object_manager.read().allocate();
        self.instructions
            .producer
            .lock()
            .push(Instruction::AddObject { handle, object });
        handle
    }

    pub fn set_object_transform(&self, handle: ObjectHandle, transform: AffineTransform) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetObjectTransform { handle, transform });
    }

    pub fn remove_object(&self, handle: ObjectHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveObject { handle })
    }

    pub fn add_directional_light(&self, light: DirectionalLight) -> DirectionalLightHandle {
        let handle = self.directional_light_manager.read().allocate();

        self.instructions
            .producer
            .lock()
            .push(Instruction::AddDirectionalLight { handle, light });

        handle
    }

    pub fn update_directional_light(&self, handle: DirectionalLightHandle, change: DirectionalLightChange) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::ChangeDirectionalLight { handle, change })
    }

    pub fn remove_directional_light(&self, handle: DirectionalLightHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveDirectionalLight { handle })
    }

    pub fn set_options(&self, options: RendererOptions) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetOptions { options })
    }

    pub fn set_camera_location(&self, location: CameraLocation) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetCameraLocation { location })
    }

    pub fn set_background_texture(&self, handle: TextureHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetBackgroundTexture { handle })
    }

    pub fn clear_background_texture(&self) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::ClearBackgroundTexture)
    }

    pub fn render(self: &Arc<Self>) -> JoinHandle<RendererStatistics> {
        self.yard
            .spawn(0, MAIN_TASK_PRIORITY, render::render_loop(Arc::clone(self)))
    }
}
