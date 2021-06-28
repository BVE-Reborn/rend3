use crate::{
    cache::{BindGroupCache, PipelineCache},
    datatypes::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material,
        MaterialChange, MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
    },
    instruction::{Instruction, InstructionStreamPair},
    modules::{DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, TextureManager},
    renderer::{info::ExtendedAdapterInfo, resources::RendererGlobalResources},
    statistics::RendererStatistics,
    util::output::RendererOutput,
    JobPriorities, RenderList, RendererBuilder, RendererInitializationError, RendererMode, RendererOptions,
};
use glam::Mat4;
use parking_lot::RwLock;
use raw_window_handle::HasRawWindowHandle;
use std::{cmp::Ordering, future::Future, sync::Arc};
use switchyard::{JoinHandle, Switchyard};
use wgpu::{Device, Instance, Queue, Surface};

pub mod error;
mod info;
pub mod limits;
mod render;
mod resources;
mod setup;

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct OrdEqFloat(pub f32);
impl Eq for OrdEqFloat {}
#[allow(clippy::derive_ord_xor_partial_ord)] // Shhh let me break your contract in peace
impl Ord for OrdEqFloat {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap_or(Ordering::Greater)
    }
}

pub struct Renderer<TLD = ()>
where
    TLD: 'static,
{
    yard: Arc<Switchyard<TLD>>,
    yard_priorites: JobPriorities,
    instructions: InstructionStreamPair,

    mode: RendererMode,
    adapter_info: ExtendedAdapterInfo,
    instance: Arc<Instance>,
    queue: Arc<Queue>,
    device: Arc<Device>,
    surface: Option<Surface>,

    global_resources: RwLock<RendererGlobalResources>,
    mesh_manager: RwLock<MeshManager>,
    texture_manager_2d: RwLock<TextureManager>,
    texture_manager_cube: RwLock<TextureManager>,
    material_manager: RwLock<MaterialManager>,
    object_manager: RwLock<ObjectManager>,
    directional_light_manager: RwLock<DirectionalLightManager>,

    pipeline_cache: RwLock<PipelineCache>,
    bind_group_cache: RwLock<BindGroupCache>,

    options: RwLock<RendererOptions>,
}
impl<TLD: 'static> Renderer<TLD> {
    /// Use [`RendererBuilder`](crate::RendererBuilder) to create a renderer.
    pub(crate) fn new<W: HasRawWindowHandle>(
        builder: RendererBuilder<'_, W, TLD>,
    ) -> impl Future<Output = Result<Arc<Self>, RendererInitializationError>> + '_ {
        setup::create_renderer(builder)
    }

    pub fn mode(&self) -> RendererMode {
        self.mode
    }

    pub fn instance(&self) -> &Arc<Instance> {
        &self.instance
    }

    pub fn device(&self) -> &Arc<Device> {
        &self.device
    }

    pub fn queue(&self) -> &Arc<Queue> {
        &self.queue
    }

    pub fn adapter_info(&self) -> ExtendedAdapterInfo {
        self.adapter_info.clone()
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

    pub fn set_object_transform(&self, handle: ObjectHandle, transform: Mat4) {
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

    pub fn set_camera_data(&self, data: Camera) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetCameraData { data })
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

    pub fn render(
        self: &Arc<Self>,
        list: Arc<dyn RenderList<TLD>>,
        output: RendererOutput,
    ) -> JoinHandle<RendererStatistics> {
        let this = Arc::clone(self);
        self.yard.spawn_local(
            self.yard_priorites.compute_pool,
            self.yard_priorites.main_task_priority,
            move |_| render::render_loop(this, list, output),
        )
    }
}
