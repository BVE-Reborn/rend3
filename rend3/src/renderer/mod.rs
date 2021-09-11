use crate::{
    instruction::{Instruction, InstructionStreamPair},
    renderer::info::ExtendedAdapterInfo,
    resources::{CameraManager, DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, TextureManager},
    types::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material, MaterialChange,
        MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
    },
    util::{mipmap::MipmapGenerator, output::OutputFrame, typedefs::RendererStatistics},
    InstanceAdapterDevice, RenderRoutine, RendererInitializationError, RendererMode,
};
use glam::Mat4;
use parking_lot::{Mutex, RwLock};
use rend3_types::TextureFromTexture;
use std::{cmp::Ordering, sync::Arc};
use wgpu::{Device, Instance, Queue};
use wgpu_profiler::GpuProfiler;

pub mod error;
pub mod info;
pub mod limits;
mod render;
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

    pub camera_manager: RwLock<CameraManager>,
    pub mesh_manager: RwLock<MeshManager>,
    pub d2_texture_manager: RwLock<TextureManager>,
    pub d2c_texture_manager: RwLock<TextureManager>,
    pub material_manager: RwLock<MaterialManager>,
    pub object_manager: RwLock<ObjectManager>,
    pub directional_light_manager: RwLock<DirectionalLightManager>,

    pub mipmap_generator: Mutex<MipmapGenerator>,

    pub profiler: Mutex<GpuProfiler>,
}
impl Renderer {
    /// Use [`RendererBuilder`](crate::RendererBuilder) to create a renderer.
    pub fn new(
        iad: InstanceAdapterDevice,
        aspect_ratio: Option<f32>,
    ) -> Result<Arc<Self>, RendererInitializationError> {
        setup::create_renderer(iad, aspect_ratio)
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

    pub fn add_texture_2d_from_texture(&self, texture: TextureFromTexture) -> TextureHandle {
        let handle = self.d2_texture_manager.read().allocate();
        self.instructions
            .producer
            .lock()
            .push(Instruction::AddTexture2DFromTexture {
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

    pub fn set_aspect_ratio(&self, ratio: f32) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetAspectRatio { ratio })
    }

    pub fn set_camera_data(&self, data: Camera) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetCameraData { data })
    }

    /// Render a frame of the scene onto the given output, using the given RenderRoutine.
    ///
    /// The RendererStatistics may not be the results from this frame, but might be the results from multiple frames ago.
    pub fn render(
        self: &Arc<Self>,
        routine: &mut dyn RenderRoutine,
        output: OutputFrame,
    ) -> Option<RendererStatistics> {
        render::render_loop(Arc::clone(self), routine, output)
    }
}
