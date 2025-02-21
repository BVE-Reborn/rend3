use std::{marker::PhantomData, panic::Location, sync::Arc};

use glam::Mat4;
use parking_lot::Mutex;
use rend3_types::{
    GraphDataHandle, GraphDataTag, Handedness, Material, MaterialTag, ObjectChange, PointLight, PointLightChange,
    PointLightHandle, Skeleton, SkeletonHandle, Texture2DTag, TextureCubeHandle, TextureCubeTag, TextureFromTexture,
    WasmNotSend,
};
use wgpu::{Device, DownlevelCapabilities, Features, Limits, Queue};
use wgpu_profiler::GpuProfiler;

use crate::{
    graph::{GraphTextureStore, InstructionEvaluationOutput},
    instruction::{InstructionKind, InstructionStreamPair},
    managers::{
        CameraState, DirectionalLightManager, GraphStorage, HandleAllocator, MaterialManager, MeshCreationError,
        MeshManager, ObjectManager, PointLightManager, SkeletonCreationError, SkeletonManager, TextureCreationError,
        TextureManager,
    },
    types::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, MaterialHandle, Mesh, MeshHandle,
        Object, ObjectHandle, Texture, Texture2DHandle,
    },
    util::{mipmap::MipmapGenerator, scatter_copy::ScatterCopy},
    ExtendedAdapterInfo, InstanceAdapterDevice, RendererInitializationError, RendererProfile,
};

pub mod error;
mod eval;
mod setup;

/// Core struct which contains the renderer world. Primary way to interact with
/// the world.
pub struct Renderer {
    pub(crate) instructions: InstructionStreamPair,

    /// The rendering profile used.
    pub profile: RendererProfile,
    /// Information about the adapter.
    pub adapter_info: ExtendedAdapterInfo,
    /// Queue all command buffers will be submitted to.
    pub queue: Arc<Queue>,
    /// Device all objects will be created with.
    pub device: Arc<Device>,

    /// Features of the device
    pub features: Features,
    /// Limits of the device
    pub limits: Limits,
    /// Downlevel limits of the device
    pub downlevel: DownlevelCapabilities,
    /// Handedness of all parts of this renderer.
    pub handedness: Handedness,

    /// Allocators for resource handles
    resource_handle_allocators: HandleAllocators,
    /// Manages all vertex and index data.
    pub mesh_manager: MeshManager,
    /// All the lockable data
    pub data_core: Mutex<RendererDataCore>,

    /// Tool which generates mipmaps from a texture.
    pub mipmap_generator: MipmapGenerator,
    /// Tool which allows scatter uploads to happen.
    pub scatter: ScatterCopy,
}

/// Handle allocators
struct HandleAllocators {
    pub mesh: HandleAllocator<Mesh>,
    pub skeleton: HandleAllocator<Skeleton>,
    pub d2_texture: HandleAllocator<Texture2DTag>,
    pub d2c_texture: HandleAllocator<TextureCubeTag>,
    pub material: HandleAllocator<MaterialTag>,
    pub object: HandleAllocator<Object>,
    pub directional_light: HandleAllocator<DirectionalLight>,
    pub point_light: HandleAllocator<PointLight>,
    pub graph_storage: HandleAllocator<GraphDataTag>,
}

impl Default for HandleAllocators {
    fn default() -> Self {
        Self {
            mesh: HandleAllocator::new(),
            skeleton: HandleAllocator::new(),
            d2_texture: HandleAllocator::new(),
            d2c_texture: HandleAllocator::new(),
            material: HandleAllocator::new(),
            object: HandleAllocator::new(),
            directional_light: HandleAllocator::new(),
            point_light: HandleAllocator::new(),
            graph_storage: HandleAllocator::new(),
        }
    }
}

/// All the mutex protected data within the renderer
pub struct RendererDataCore {
    /// Position and settings of the viewport camera.
    pub viewport_camera_state: CameraState,
    /// Manages all 2D textures, including bindless bind group.
    pub d2_texture_manager: TextureManager<Texture2DTag>,
    /// Manages all Cube textures, including bindless bind groups.
    pub d2c_texture_manager: TextureManager<TextureCubeTag>,
    /// Manages all materials, including material bind groups when CpuDriven.
    pub material_manager: MaterialManager,
    /// Manages all objects.
    pub object_manager: ObjectManager,
    /// Manages all directional lights, including their shadow maps.
    pub directional_light_manager: DirectionalLightManager,
    /// Manages all point lights, including their shadow maps.
    pub point_light_manager: PointLightManager,
    /// Manages skeletons, and their owned portion of the MeshManager's buffers
    pub skeleton_manager: SkeletonManager,
    /// Managed long term storage of data for the graph and it's routines
    pub graph_storage: GraphStorage,

    /// Stores gpu timing and debug scopes.
    pub profiler: Mutex<GpuProfiler>,

    /// Stores a cache of render targets between graph invocations.
    pub(crate) graph_texture_store: GraphTextureStore,
}

impl Renderer {
    /// Create a new renderer with the given IAD.
    ///
    /// You can create your own IAD or call [`create_iad`](crate::create_iad).
    ///
    /// The aspect ratio is that of the window. This automatically configures
    /// the camera. If None is passed, an aspect ratio of 1.0 is assumed.
    pub fn new(
        iad: InstanceAdapterDevice,
        handedness: Handedness,
        aspect_ratio: Option<f32>,
    ) -> Result<Arc<Self>, RendererInitializationError> {
        setup::create_renderer(iad, handedness, aspect_ratio)
    }

    /// Adds a 3D mesh to the renderer. This doesn't instantiate it to world. To
    /// show this in the world, you need to create an [`Object`] using this
    /// mesh.
    ///
    /// The handle will keep the mesh alive. All objects created will also keep
    /// the mesh alive.
    #[track_caller]
    pub fn add_mesh(self: &Arc<Self>, mesh: Mesh) -> Result<MeshHandle, MeshCreationError> {
        let internal_mesh = self.mesh_manager.add(&self.device, mesh)?;

        // Handle allocation must be done _after_ any validation to prevent deletion of a handle that never gets fully added.
        let handle = self.resource_handle_allocators.mesh.allocate(self);

        self.mesh_manager.fill(&handle, internal_mesh);

        Ok(handle)
    }

    /// Adds a skeleton into the renderer. This combines a [`Mesh`] with a set
    /// of joints that can be used to animate that mesh.
    ///
    /// The handle will keep the skeleton alive. All objects created will also
    /// keep the skeleton alive. The skeleton will also keep the mesh it
    /// references alive.
    #[track_caller]
    pub fn add_skeleton(self: &Arc<Self>, skeleton: Skeleton) -> Result<SkeletonHandle, SkeletonCreationError> {
        let internal = SkeletonManager::validate_skeleton(&self.device, &self.mesh_manager, skeleton)?;

        // Handle allocation must be done _after_ any validation to prevent deletion of a handle that never gets fully added.
        let handle = self.resource_handle_allocators.skeleton.allocate(self);

        self.instructions
            .push(InstructionKind::AddSkeleton { handle: *handle, skeleton: Box::new(internal) }, *Location::caller());

        Ok(handle)
    }

    /// Add a 2D texture to the renderer. This can be used in a [`Material`].
    ///
    /// The handle will keep the texture alive. All materials created with this
    /// texture will also keep the texture alive.
    #[track_caller]
    pub fn add_texture_2d(self: &Arc<Self>, texture: Texture) -> Result<Texture2DHandle, TextureCreationError> {
        profiling::scope!("Add Texture 2D");

        let (cmd_buf, internal_texture) = TextureManager::<Texture2DTag>::add(self, texture, false)?;

        // Handle allocation must be done _after_ any validation to prevent deletion of a handle that never gets fully added.
        let handle = self.resource_handle_allocators.d2_texture.allocate(self);

        self.instructions
            .push(InstructionKind::AddTexture2D { handle: *handle, internal_texture, cmd_buf }, *Location::caller());

        Ok(handle)
    }

    /// Add a 2D texture to the renderer by copying a set of mipmaps from an
    /// existing texture. This new can be used in a [`Material`].
    ///
    /// The handle will keep the texture alive. All materials created with this
    /// texture will also keep the texture alive.
    #[track_caller]
    pub fn add_texture_2d_from_texture(self: &Arc<Self>, texture: TextureFromTexture) -> Texture2DHandle {
        profiling::scope!("Add Texture 2D From Texture");

        let handle = self.resource_handle_allocators.d2_texture.allocate(self);

        self.instructions
            .push(InstructionKind::AddTexture2DFromTexture { handle: *handle, texture }, *Location::caller());

        handle
    }

    /// Adds a Cube texture to the renderer. This can be used as a cube
    /// environment map by a render routine.
    ///
    /// The handle will keep the texture alive.
    #[track_caller]
    pub fn add_texture_cube(self: &Arc<Self>, texture: Texture) -> Result<TextureCubeHandle, TextureCreationError> {
        profiling::scope!("Add Texture Cube");

        let (cmd_buf, internal_texture) = TextureManager::<TextureCubeTag>::add(self, texture, true)?;

        // Handle allocation must be done _after_ any validation to prevent deletion of a handle that never gets fully added.
        let handle = self.resource_handle_allocators.d2c_texture.allocate(self);

        self.instructions
            .push(InstructionKind::AddTextureCube { handle: *handle, internal_texture, cmd_buf }, *Location::caller());

        Ok(handle)
    }

    /// Adds a material to the renderer. This can be used in an [`Object`].
    ///
    /// The handle will keep the material alive. All objects created with this
    /// material will also keep this material alive.
    ///
    /// The material will keep the inside textures alive.
    #[track_caller]
    pub fn add_material<M: Material>(self: &Arc<Self>, material: M) -> MaterialHandle {
        let handle = self.resource_handle_allocators.material.allocate(self);
        self.instructions.push(
            InstructionKind::AddMaterial {
                handle: *handle,
                fill_invoke: Box::new(move |material_manager, device, profile, d2_manager, mat_handle| {
                    material_manager.add(device, profile, d2_manager, &mat_handle, material)
                }),
            },
            *Location::caller(),
        );
        handle
    }

    /// Updates a given material. Old references will be dropped.
    #[track_caller]
    pub fn update_material<M: Material>(&self, handle: &MaterialHandle, material: M) {
        self.instructions.push(
            InstructionKind::ChangeMaterial {
                handle: **handle,
                change_invoke: Box::new(move |material_manager, device, d2_manager, mat_handle| {
                    material_manager.update(device, d2_manager, &mat_handle, material)
                }),
            },
            *Location::caller(),
        )
    }

    /// Adds an object to the renderer. This will create a visible object using
    /// the given mesh and materal.
    ///
    /// The handle will keep the material alive.
    ///
    /// The object will keep all materials, textures, and meshes alive.
    #[track_caller]
    pub fn add_object(self: &Arc<Self>, object: Object) -> ObjectHandle {
        let handle = self.resource_handle_allocators.object.allocate(self);
        self.instructions.push(InstructionKind::AddObject { handle: *handle, object }, *Location::caller());
        handle
    }

    /// Duplicates an existing object in the renderer, returning the new
    /// object's handle. Any changes specified in the `change` struct will be
    /// applied to the duplicated object, and the same mesh, material and
    /// transform as the original object will be used otherwise.
    #[track_caller]
    pub fn duplicate_object(self: &Arc<Self>, object_handle: &ObjectHandle, change: ObjectChange) -> ObjectHandle {
        let dst_handle = self.resource_handle_allocators.object.allocate(self);
        self.instructions.push(
            InstructionKind::DuplicateObject { src_handle: **object_handle, dst_handle: *dst_handle, change },
            *Location::caller(),
        );
        dst_handle
    }

    /// Move the given object to a new transform location.
    #[track_caller]
    pub fn set_object_transform(&self, handle: &ObjectHandle, transform: Mat4) {
        //  Note copy of raw handle, which will live as long as the instruction set.
        self.instructions
            .push(InstructionKind::SetObjectTransform { handle: *handle.get_raw(), transform }, *Location::caller());
    }

    /// Sets the joint positions for a skeleton. See
    /// [Renderer::set_skeleton_joint_matrices] to set the vertex
    /// transformations directly, without having to supply two separate
    /// matrix vectors.
    ///
    /// ## Inputs
    /// - `joint_global_positions`: Contains one transform matrix per bone,
    ///   containing that bone's current clobal transform
    /// - `inverse_bind_poses`: Contains one inverse bind transform matrix per
    ///   bone, that is, the inverse of the bone's transformation at its rest
    ///   position.
    #[track_caller]
    pub fn set_skeleton_joint_transforms(
        &self,
        handle: &SkeletonHandle,
        joint_global_transforms: &[Mat4],
        inverse_bind_transforms: &[Mat4],
    ) {
        self.set_skeleton_joint_matrices(
            handle,
            Skeleton::compute_joint_matrices(joint_global_transforms, inverse_bind_transforms),
        );
    }

    /// Sets the joint matrices for a skeleton. The joint matrix is the
    /// transformation that will be applied to a vertex affected by a joint.
    /// Note that this is not the same as the joint's transformation. See
    /// [Renderer::set_skeleton_joint_transforms] for an alternative method that
    /// allows setting the joint transformation instead.
    #[track_caller]
    pub fn set_skeleton_joint_matrices(&self, handle: &SkeletonHandle, joint_matrices: Vec<Mat4>) {
        self.instructions.push(
            InstructionKind::SetSkeletonJointDeltas { handle: *handle.get_raw(), joint_matrices },
            *Location::caller(),
        )
    }

    /// Add a sun-like light into the world.
    ///
    /// The handle will keep the light alive.
    #[track_caller]
    pub fn add_directional_light(self: &Arc<Self>, light: DirectionalLight) -> DirectionalLightHandle {
        let handle = self.resource_handle_allocators.directional_light.allocate(self);

        self.instructions.push(InstructionKind::AddDirectionalLight { handle: *handle, light }, *Location::caller());

        handle
    }

    /// Add a point (aka punctual) light into the world.
    ///
    /// **WARNING**: point lighting is currently very inefficient, as every
    /// fragment in the forward pass is shaded with every point light in the
    /// world.
    ///
    /// The handle will keep the light alive.
    #[track_caller]
    pub fn add_point_light(self: &Arc<Self>, light: PointLight) -> PointLightHandle {
        let handle = self.resource_handle_allocators.point_light.allocate(self);

        self.instructions.push(InstructionKind::AddPointLight { handle: *handle, light }, *Location::caller());

        handle
    }

    /// Updates the settings for given directional light.
    #[track_caller] 
    pub fn update_directional_light(&self, handle: &DirectionalLightHandle, change: DirectionalLightChange) {
        //  Note copy of raw handle, which will live as long as the instruction set.
        self.instructions
            .push(InstructionKind::ChangeDirectionalLight { handle: *handle.get_raw(), change }, *Location::caller())
    }

    /// Updates the settings for given point light.
    #[track_caller]
    pub fn update_point_light(&self, handle: &PointLightHandle, change: PointLightChange) {
        self.instructions
            .push(InstructionKind::ChangePointLight { handle: *handle.get_raw(), change }, *Location::caller())
    }

    /// Adds a piece of data for long term storage and convienient use in the RenderGraph
    ///
    /// The handle will keep the data alive.
    #[track_caller]
    pub fn add_graph_data<T: WasmNotSend + 'static>(self: &Arc<Renderer>, data: T) -> GraphDataHandle<T> {
        let handle = self.resource_handle_allocators.graph_storage.allocate(self);
        let handle2 = *handle;
        self.instructions.push(
            InstructionKind::AddGraphData { add_invoke: Box::new(move |storage| storage.add(&handle2, data)) },
            *Location::caller(),
        );
        GraphDataHandle(handle, PhantomData)
    }

    /// Sets the aspect ratio of the camera. This should correspond with the
    /// aspect ratio of the user.
    #[track_caller]
    pub fn set_aspect_ratio(&self, ratio: f32) {
        self.instructions.push(InstructionKind::SetAspectRatio { ratio }, *Location::caller())
    }

    /// Sets the position, pov, or projection mode of the camera.
    #[track_caller]
    pub fn set_camera_data(&self, data: Camera) {
        self.instructions.push(InstructionKind::SetCameraData { data }, *Location::caller())
    }

    /// Swaps the front and back instruction buffer. Any world-modifiying functions
    /// called after this will be recorded for the next frame.
    ///
    /// Call before [`Self::evaluate_instructions`].
    pub fn swap_instruction_buffers(&self) {
        self.instructions.swap()
    }

    /// Evaluates all instructions in the "front" buffer.
    ///
    /// After you've recorded all your instructions from world-modifying functions
    /// call [`Self::swap_instruction_buffers`] to make swap buffers. Then call
    /// this function to evaluate all of those instructions.
    pub fn evaluate_instructions(&self) -> InstructionEvaluationOutput {
        eval::evaluate_instructions(self)
    }
}
