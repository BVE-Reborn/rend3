use crate::{
    datatypes::{
        AffineTransform, CameraLocation, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material,
        MaterialChange, MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Pipeline, PipelineHandle, ShaderHandle,
        Texture, TextureHandle,
    },
    instruction::{Instruction, InstructionStreamPair},
    list::{RenderList, SourceShaderDescriptor},
    renderer::{
        info::ExtendedAdapterInfo, material::MaterialManager, mesh::MeshManager, object::ObjectManager,
        pipeline::PipelineManager, resources::RendererGlobalResources, shaders::ShaderManager, texture::TextureManager,
    },
    statistics::RendererStatistics,
    RendererInitializationError, RendererOptions,
};
pub use material::{MATERIALS_SIZE, MAX_MATERIALS};
use parking_lot::{Mutex, RwLock};
use raw_window_handle::HasRawWindowHandle;
use std::{future::Future, sync::Arc};
use switchyard::{JoinHandle, Switchyard};
use wgpu::{Backend, Device, Queue, Surface, TextureFormat};
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
mod list {
    mod cache;
    mod forward;
    mod resource;

    pub(crate) use cache::*;
    pub(crate) use forward::*;
    pub use resource::*;
}
mod material;
mod mesh;
mod object;
mod passes {
    mod culling;

    pub use culling::*;
}
mod pipeline;
mod render;
mod resources;
mod setup;
mod shaders;
mod texture;
mod uniforms;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RendererMode {
    CPUPowered,
    GPUPowered,
}

impl RendererMode {
    pub(crate) fn into_data<C, G>(self, cpu: impl FnOnce() -> C, gpu: impl FnOnce() -> G) -> ModeData<C, G> {
        match self {
            Self::CPUPowered => ModeData::CPU(cpu()),
            Self::GPUPowered => ModeData::GPU(gpu()),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum ModeData<C, G> {
    CPU(C),
    GPU(G),
}
impl<C, G> ModeData<C, G> {
    pub fn mode(&self) -> RendererMode {
        match self {
            Self::CPU(_) => RendererMode::CPUPowered,
            Self::GPU(_) => RendererMode::GPUPowered,
        }
    }

    pub fn into_cpu(self) -> C {
        match self {
            Self::CPU(c) => c,
            Self::GPU(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn as_cpu(&self) -> &C {
        match self {
            Self::CPU(c) => c,
            Self::GPU(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn as_cpu_mut(&mut self) -> &mut C {
        match self {
            Self::CPU(c) => c,
            Self::GPU(_) => panic!("tried to extract gpu data in cpu mode"),
        }
    }

    pub fn into_gpu(self) -> G {
        match self {
            Self::GPU(g) => g,
            Self::CPU(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn as_gpu(&self) -> &G {
        match self {
            Self::GPU(g) => g,
            Self::CPU(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn as_gpu_mut(&mut self) -> &mut G {
        match self {
            Self::GPU(g) => g,
            Self::CPU(_) => panic!("tried to extract cpu data in gpu mode"),
        }
    }

    pub fn as_ref(&self) -> ModeData<&C, &G> {
        match self {
            Self::CPU(c) => ModeData::CPU(c),
            Self::GPU(c) => ModeData::GPU(c),
        }
    }

    pub fn as_ref_mut(&mut self) -> ModeData<&mut C, &mut G> {
        match self {
            Self::CPU(c) => ModeData::CPU(c),
            Self::GPU(c) => ModeData::GPU(c),
        }
    }

    pub fn map_cpu<C2>(self, func: impl FnOnce(C) -> C2) -> ModeData<C2, G> {
        match self {
            Self::CPU(c) => ModeData::CPU(func(c)),
            Self::GPU(g) => ModeData::GPU(g),
        }
    }

    pub fn map_gpu<G2>(self, func: impl FnOnce(G) -> G2) -> ModeData<C, G2> {
        match self {
            Self::CPU(c) => ModeData::CPU(c),
            Self::GPU(g) => ModeData::GPU(func(g)),
        }
    }
}

const COMPUTE_POOL: u8 = 0;

const BUFFER_RECALL_PRIORITY: u32 = 0;
const MAIN_TASK_PRIORITY: u32 = 1;
const RENDER_RECORD_PRIORITY: u32 = 2;
const PIPELINE_BUILD_PRIORITY: u32 = 3;

const INTERNAL_SHADOW_DEPTH_FORMAT: TextureFormat = TextureFormat::Depth32Float;
const SWAPCHAIN_FORMAT: TextureFormat = TextureFormat::Bgra8UnormSrgb;

const SHADOW_DIMENSIONS: u32 = 2048;

pub struct Renderer<TLD = ()>
where
    TLD: 'static,
{
    yard: Arc<Switchyard<TLD>>,
    instructions: InstructionStreamPair,

    mode: RendererMode,
    _adapter_info: ExtendedAdapterInfo,
    queue: Queue,
    device: Arc<Device>,
    surface: Surface,

    buffer_manager: Mutex<AutomatedBufferManager>,
    global_resources: RwLock<RendererGlobalResources>,
    shader_manager: Arc<ShaderManager>,
    pipeline_manager: Arc<PipelineManager>,
    mesh_manager: RwLock<MeshManager>,
    texture_manager_2d: RwLock<TextureManager>,
    texture_manager_cube: RwLock<TextureManager>,
    material_manager: RwLock<MaterialManager>,
    object_manager: RwLock<ObjectManager>,
    directional_light_manager: RwLock<light::DirectionalLightManager>,

    render_list_cache: RwLock<list::RenderListCache>,

    culling_pass: passes::CullingPass,

    // _imgui_renderer: imgui_wgpu::Renderer,
    options: RwLock<RendererOptions>,
}
impl<TLD: 'static> Renderer<TLD> {
    pub fn new<'a, W: HasRawWindowHandle>(
        window: &'a W,
        yard: Arc<Switchyard<TLD>>,
        imgui_context: &'a mut imgui::Context,
        backend: Option<Backend>,
        device: Option<String>,
        options: RendererOptions,
    ) -> impl Future<Output = Result<Arc<Self>, RendererInitializationError>> + 'a {
        setup::create_renderer(window, yard, imgui_context, backend, device, options)
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

    pub fn remove_texture_cube(&self, handle: TextureHandle) {
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

    pub fn add_binary_shader(&self, shader: Vec<u32>) -> ShaderHandle {
        let handle = self.shader_manager.allocate();

        self.instructions
            .producer
            .lock()
            .push(Instruction::AddBinaryShader { handle, shader });

        handle
    }

    pub fn add_source_shader(&self, shader: SourceShaderDescriptor) -> impl Future<Output = ShaderHandle> {
        self.shader_manager.allocate_async_insert(shader)
    }

    pub fn remove_shader(&self, handle: ShaderHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemoveShader { handle });
    }

    pub fn add_pipeline(self: &Arc<Self>, pipeline: Pipeline) -> impl Future<Output = PipelineHandle> {
        self.pipeline_manager.allocate_async_insert(Arc::clone(self), pipeline)
    }

    pub fn remove_pipeline(&self, handle: PipelineHandle) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::RemovePipeline { handle });
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

    pub fn render(self: &Arc<Self>, list: RenderList) -> JoinHandle<RendererStatistics> {
        self.yard
            .spawn(0, MAIN_TASK_PRIORITY, render::render_loop(Arc::clone(self), list))
    }
}
