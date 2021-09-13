use crate::{
    instruction::{Instruction, InstructionStreamPair},
    resources::{CameraManager, DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, TextureManager},
    types::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, Material, MaterialChange,
        MaterialHandle, Mesh, MeshHandle, Object, ObjectHandle, Texture, TextureHandle,
    },
    util::{mipmap::MipmapGenerator, output::OutputFrame, typedefs::RendererStatistics},
    ExtendedAdapterInfo, InstanceAdapterDevice, RenderRoutine, RendererInitializationError, RendererMode,
};
use glam::Mat4;
use parking_lot::{Mutex, RwLock};
use rend3_types::TextureFromTexture;
use std::sync::Arc;
use wgpu::{Device, Queue};
use wgpu_profiler::GpuProfiler;

pub mod error;
mod render;
mod setup;

/// Core struct which contains the renderer world. Primary way to interact with the world.
pub struct Renderer {
    instructions: InstructionStreamPair,

    /// The culling mode used.
    pub mode: RendererMode,
    /// Information about the adapter.
    pub adapter_info: ExtendedAdapterInfo,
    /// Queue all command buffers will be submitted to.
    pub queue: Arc<Queue>,
    /// Device all objects will be created with.
    pub device: Arc<Device>,

    /// Position and settings of the camera.
    pub camera_manager: RwLock<CameraManager>,
    /// Manages all vertex and index data.
    pub mesh_manager: RwLock<MeshManager>,
    /// Manages all 2D textures, including bindless bind group.
    pub d2_texture_manager: RwLock<TextureManager>,
    /// Manages all Cube textures, including bindless bind groups.
    pub d2c_texture_manager: RwLock<TextureManager>,
    /// Manages all materials, including material bind groups in CPU mode.
    pub material_manager: RwLock<MaterialManager>,
    /// Manages all objects.
    pub object_manager: RwLock<ObjectManager>,
    /// Manages all directional lights, including their shadow maps.
    pub directional_light_manager: RwLock<DirectionalLightManager>,

    pub mipmap_generator: Mutex<MipmapGenerator>,

    /// Stores gpu timing and debug scopes.
    pub profiler: Mutex<GpuProfiler>,
}
impl Renderer {
    /// Create a new renderer with the given IAD.
    ///
    /// You can create your own IAD or call [`create_iad`](crate::create_iad).
    ///
    /// The aspect ratio is that of the window. This automatically configures the camera. If None is passed, an aspect ratio of 1.0 is assumed.
    pub fn new(
        iad: InstanceAdapterDevice,
        aspect_ratio: Option<f32>,
    ) -> Result<Arc<Self>, RendererInitializationError> {
        setup::create_renderer(iad, aspect_ratio)
    }

    /// Adds a 3D mesh to the renderer. This doesn't instantiate it to world. To show this in the world, you need to create an [`Object`] using this mesh.
    ///
    /// The handle will keep the mesh alive. All objects created will also keep the mesh alive.
    pub fn add_mesh(&self, mesh: Mesh) -> MeshHandle {
        let handle = self.mesh_manager.read().allocate();

        self.instructions.producer.lock().push(Instruction::AddMesh {
            handle: handle.clone(),
            mesh,
        });

        handle
    }

    /// Add a 2D texture to the renderer. This can be used in a [`Material`].
    ///
    /// The handle will keep the texture alive. All materials created with this texture will also keep the texture alive.
    pub fn add_texture_2d(&self, texture: Texture) -> TextureHandle {
        let handle = self.d2_texture_manager.read().allocate();
        self.instructions.producer.lock().push(Instruction::AddTexture2D {
            handle: handle.clone(),
            texture,
        });
        handle
    }

    /// Add a 2D texture to the renderer by copying a set of mipmaps from an existing texture. This new can be used in a [`Material`].
    ///
    /// The handle will keep the texture alive. All materials created with this texture will also keep the texture alive.
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

    /// Adds a Cube texture to the renderer. This can be used as a cube environment map by a render routine.
    ///
    /// The handle will keep the texture alive.
    pub fn add_texture_cube(&self, texture: Texture) -> TextureHandle {
        let handle = self.d2c_texture_manager.read().allocate();
        self.instructions.producer.lock().push(Instruction::AddTextureCube {
            handle: handle.clone(),
            texture,
        });
        handle
    }

    /// Adds a material to the renderer. This can be used in an [`Object`].
    ///
    /// The handle will keep the material alive. All objects created with this material will also keep this material alive.
    ///
    /// The material will keep the inside textures alive.
    pub fn add_material(&self, material: Material) -> MaterialHandle {
        let handle = self.material_manager.read().allocate();
        self.instructions.producer.lock().push(Instruction::AddMaterial {
            handle: handle.clone(),
            material,
        });
        handle
    }

    /// Updates a given material. Old references will be dropped.
    pub fn update_material(&self, handle: &MaterialHandle, change: MaterialChange) {
        self.instructions.producer.lock().push(Instruction::ChangeMaterial {
            handle: handle.get_raw(),
            change,
        })
    }

    /// Adds an object to the renderer. This will create a visible object using the given mesh and materal.
    ///
    /// The handle will keep the material alive.
    ///
    /// The object will keep all materials, textures, and meshes alive.
    pub fn add_object(&self, object: Object) -> ObjectHandle {
        let handle = self.object_manager.read().allocate();
        self.instructions.producer.lock().push(Instruction::AddObject {
            handle: handle.clone(),
            object,
        });
        handle
    }

    /// Move the given object to a new transform location.
    pub fn set_object_transform(&self, handle: &ObjectHandle, transform: Mat4) {
        self.instructions.producer.lock().push(Instruction::SetObjectTransform {
            handle: handle.get_raw(),
            transform,
        });
    }

    /// Add a sun-like light into the world.
    ///
    /// The handle will keep the light alive.
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

    /// Updates the settings for given directional light.
    pub fn update_directional_light(&self, handle: &DirectionalLightHandle, change: DirectionalLightChange) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::ChangeDirectionalLight {
                handle: handle.get_raw(),
                change,
            })
    }

    /// Sets the aspect ratio of the camera. This should correspond with the aspect ratio of the user.
    pub fn set_aspect_ratio(&self, ratio: f32) {
        self.instructions
            .producer
            .lock()
            .push(Instruction::SetAspectRatio { ratio })
    }

    /// Sets the position, pov, or projection mode of the camera.
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
