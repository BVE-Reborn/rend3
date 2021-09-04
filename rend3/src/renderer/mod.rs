use crate::{
    instruction::{Instruction, InstructionStreamPair},
    renderer::{info::ExtendedAdapterInfo, resources::RendererGlobalResources},
    resources::{DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, TextureManager},
    types::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material, MaterialChange,
        MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
    },
    util::{output::RendererOutput, typedefs::RendererStatistics},
    InternalSurfaceOptions, RenderRoutine, RendererBuilder, RendererInitializationError, RendererMode,
};
use glam::Mat4;
use parking_lot::{Mutex, RwLock};
use raw_window_handle::HasRawWindowHandle;
use std::{cmp::Ordering, future::Future, sync::Arc};
use wgpu::{Device, Instance, Queue, Surface};
use wgpu_profiler::GpuProfiler;

pub mod error;
pub mod info;
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

pub struct Renderer {
    instructions: InstructionStreamPair,

    pub mode: RendererMode,
    pub adapter_info: ExtendedAdapterInfo,
    pub instance: Arc<Instance>,
    pub queue: Arc<Queue>,
    pub device: Arc<Device>,
    pub surface: Option<Surface>,

    pub global_resources: RwLock<RendererGlobalResources>,
    pub mesh_manager: RwLock<MeshManager>,
    pub d2_texture_manager: RwLock<TextureManager>,
    pub d2c_texture_manager: RwLock<TextureManager>,
    pub material_manager: RwLock<MaterialManager>,
    pub object_manager: RwLock<ObjectManager>,
    pub directional_light_manager: RwLock<DirectionalLightManager>,

    pub profiler: Mutex<GpuProfiler>,

    options: RwLock<InternalSurfaceOptions>,
}
impl Renderer {
    /// Use [`RendererBuilder`](crate::RendererBuilder) to create a renderer.
    pub(crate) fn new<W: HasRawWindowHandle>(
        builder: RendererBuilder<'_, W>,
    ) -> impl Future<Output = Result<Arc<Self>, RendererInitializationError>> + '_ {
        setup::create_renderer(builder)
    }

    pub fn add_mesh(&self, mesh: Mesh) -> MeshHandle {
        let handle = self.mesh_manager.read().allocate();

        self.instructions.producer.lock().push(Instruction::AddMesh {
            handle: handle.clone(),
            mesh,
        });

        handle
    }

    pub fn add_texture_2d(&self, texture: Texture) -> TextureHandle {
        let handle = self.d2_texture_manager.read().allocate();
        self.instructions.producer.lock().push(Instruction::AddTexture2D {
            handle: handle.clone(),
            texture,
        });
        handle
    }

    pub fn add_texture_cube(&self, texture: Texture) -> TextureHandle {
        let handle = self.d2c_texture_manager.read().allocate();
        self.instructions.producer.lock().push(Instruction::AddTextureCube {
            handle: handle.clone(),
            texture,
        });
        handle
    }

    pub fn add_material(&self, material: Material) -> MaterialHandle {
        let handle = self.material_manager.read().allocate();
        self.instructions.producer.lock().push(Instruction::AddMaterial {
            handle: handle.clone(),
            material,
        });
        handle
    }

    pub fn update_material(&self, handle: &MaterialHandle, change: MaterialChange) {
        self.instructions.producer.lock().push(Instruction::ChangeMaterial {
            handle: handle.get_raw(),
            change,
        })
    }

    pub fn add_object(&self, object: Object) -> ObjectHandle {
        let handle = self.object_manager.read().allocate();
        self.instructions.producer.lock().push(Instruction::AddObject {
            handle: handle.clone(),
            object,
        });
        handle
    }

    pub fn set_object_transform(&self, handle: &ObjectHandle, transform: Mat4) {
        self.instructions.producer.lock().push(Instruction::SetObjectTransform {
            handle: handle.get_raw(),
            transform,
        });
    }

    pub fn add_directional_light(&self, light: DirectionalLight) -> DirectionalLightHandle {
        let handle = self.directional_light_manager.read().allocate();

        self.instructions
            .producer
            .lock()
            .push(Instruction::AddDirectionalLight {
                handle: handle.clone(),
                light,
            });

        handle
    }

    pub fn update_directional_light(&self, handle: &DirectionalLightHandle, change: DirectionalLightChange) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::ChangeDirectionalLight {
                handle: handle.get_raw(),
                change,
            })
    }

    pub fn set_internal_surface_options(&self, options: InternalSurfaceOptions) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetInternalSurfaceOptions { options })
    }

    pub fn set_camera_data(&self, data: Camera) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetCameraData { data })
    }

    pub fn render(
        self: &Arc<Self>,
        routine: &mut dyn RenderRoutine,
        output: RendererOutput,
    ) -> Option<RendererStatistics> {
        render::render_loop(Arc::clone(self), routine, output)
    }
}
