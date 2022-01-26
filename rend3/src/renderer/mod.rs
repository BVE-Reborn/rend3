use crate::{
    format_sso,
    graph::{GraphTextureStore, ReadyData},
    instruction::{InstructionKind, InstructionStreamPair},
    managers::{
        CameraManager, DirectionalLightManager, InternalTexture, MaterialManager, MeshManager, ObjectManager,
        SkeletonManager, TextureManager,
    },
    types::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, MaterialHandle, Mesh, MeshHandle,
        Object, ObjectHandle, Texture, TextureHandle,
    },
    util::mipmap::MipmapGenerator,
    ExtendedAdapterInfo, InstanceAdapterDevice, RendererInitializationError, RendererProfile,
};
use glam::Mat4;
use parking_lot::Mutex;
use rend3_types::{
    Handedness, Material, MipmapCount, MipmapSource, ObjectChange, Skeleton, SkeletonHandle, TextureFormat,
    TextureFromTexture, TextureUsages,
};
use std::{
    num::NonZeroU32,
    panic::Location,
    sync::{atomic::AtomicUsize, Arc},
};
use wgpu::{
    util::DeviceExt, CommandBuffer, CommandEncoderDescriptor, Device, DownlevelCapabilities, Extent3d, Features,
    ImageCopyTexture, ImageDataLayout, Limits, Origin3d, Queue, TextureAspect, TextureDescriptor, TextureDimension,
    TextureSampleType, TextureViewDescriptor, TextureViewDimension,
};
use wgpu_profiler::GpuProfiler;

pub mod error;
mod ready;
mod setup;

/// Core struct which contains the renderer world. Primary way to interact with
/// the world.
pub struct Renderer {
    instructions: InstructionStreamPair,

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

    /// Identifier allocator.
    current_ident: AtomicUsize,
    /// All the lockable data
    pub data_core: Mutex<RendererDataCore>,

    /// Tool which generates mipmaps from a texture.
    pub mipmap_generator: MipmapGenerator,
}

/// All the mutex protected data within the renderer
pub struct RendererDataCore {
    /// Position and settings of the camera.
    pub camera_manager: CameraManager,
    /// Manages all vertex and index data.
    pub mesh_manager: MeshManager,
    /// Manages all 2D textures, including bindless bind group.
    pub d2_texture_manager: TextureManager,
    /// Manages all Cube textures, including bindless bind groups.
    pub d2c_texture_manager: TextureManager,
    /// Manages all materials, including material bind groups when CpuDriven.
    pub material_manager: MaterialManager,
    /// Manages all objects.
    pub object_manager: ObjectManager,
    /// Manages all directional lights, including their shadow maps.
    pub directional_light_manager: DirectionalLightManager,
    /// Manages skeletons, and their owned portion of the MeshManager's buffers
    pub skeleton_manager: SkeletonManager,

    /// Stores gpu timing and debug scopes.
    pub profiler: GpuProfiler,

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
    pub fn add_mesh(&self, mesh: Mesh) -> MeshHandle {
        let handle = MeshManager::allocate(&self.current_ident);

        self.instructions.push(
            InstructionKind::AddMesh {
                handle: handle.clone(),
                mesh,
            },
            *Location::caller(),
        );

        handle
    }

    /// Adds a skeleton into the renderer. This combines a [`Mesh`] with a set
    /// of joints that can be used to animate that mesh.
    ///
    /// The handle will keep the skeleton alive. All objects created will also
    /// keep the skeleton alive. The skeleton will also keep the mesh it
    /// references alive.
    #[track_caller]
    pub fn add_skeleton(&self, skeleton: Skeleton) -> SkeletonHandle {
        let handle = SkeletonManager::allocate(&self.current_ident);
        self.instructions.push(
            InstructionKind::AddSkeleton {
                handle: handle.clone(),
                skeleton,
            },
            *Location::caller(),
        );
        handle
    }

    /// Add a 2D texture to the renderer. This can be used in a [`Material`].
    ///
    /// The handle will keep the texture alive. All materials created with this
    /// texture will also keep the texture alive.
    #[track_caller]
    pub fn add_texture_2d(&self, texture: Texture) -> TextureHandle {
        profiling::scope!("Add Texture 2D");

        Self::validation_texture_format(texture.format);

        let handle = TextureManager::allocate(&self.current_ident);
        let size = Extent3d {
            width: texture.size.x,
            height: texture.size.y,
            depth_or_array_layers: 1,
        };

        let mip_level_count = match texture.mip_count {
            MipmapCount::Specific(v) => v.get(),
            MipmapCount::Maximum => size.max_mips(),
        };

        let desc = TextureDescriptor {
            label: None,
            size,
            mip_level_count,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: texture.format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
        };

        let (buffer, tex) = match texture.mip_source {
            MipmapSource::Uploaded => (
                None,
                self.device.create_texture_with_data(&self.queue, &desc, &texture.data),
            ),
            MipmapSource::Generated => {
                let desc = TextureDescriptor {
                    usage: desc.usage | TextureUsages::RENDER_ATTACHMENT,
                    ..desc
                };
                let tex = self.device.create_texture(&desc);

                let format_desc = texture.format.describe();

                // write first level
                self.queue.write_texture(
                    ImageCopyTexture {
                        texture: &tex,
                        mip_level: 0,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    &texture.data,
                    ImageDataLayout {
                        offset: 0,
                        bytes_per_row: NonZeroU32::new(
                            format_desc.block_size as u32 * (size.width / format_desc.block_dimensions.0 as u32),
                        ),
                        rows_per_image: None,
                    },
                    size,
                );

                let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor::default());

                // generate mipmaps
                self.mipmap_generator
                    .generate_mipmaps(&self.device, &mut encoder, &tex, &desc);

                (Some(encoder.finish()), tex)
            }
        };

        let view = tex.create_view(&TextureViewDescriptor::default());
        self.instructions.push(
            InstructionKind::AddTexture {
                handle: handle.clone(),
                desc,
                texture: tex,
                view,
                buffer,
                cube: false,
            },
            *Location::caller(),
        );
        handle
    }

    /// Add a 2D texture to the renderer by copying a set of mipmaps from an
    /// existing texture. This new can be used in a [`Material`].
    ///
    /// The handle will keep the texture alive. All materials created with this
    /// texture will also keep the texture alive.
    #[track_caller]
    pub fn add_texture_2d_from_texture(&self, texture: TextureFromTexture) -> TextureHandle {
        profiling::scope!("Add Texture 2D From Texture");

        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor::default());

        let handle = TextureManager::allocate(&self.current_ident);

        let data_core = self.data_core.lock();

        let InternalTexture {
            texture: old_texture,
            desc: old_texture_desc,
        } = data_core.d2_texture_manager.get_internal(texture.src.get_raw());

        let new_size = old_texture_desc.mip_level_size(texture.start_mip).unwrap();

        let mip_level_count = texture
            .mip_count
            .map_or_else(|| old_texture_desc.mip_level_count - texture.start_mip, |c| c.get());

        let desc = TextureDescriptor {
            size: new_size,
            mip_level_count,
            ..old_texture_desc.clone()
        };

        let tex = self.device.create_texture(&desc);

        let view = tex.create_view(&TextureViewDescriptor::default());

        for new_mip in 0..mip_level_count {
            let old_mip = new_mip + texture.start_mip;

            let _label = format_sso!("mip {} to {}", old_mip, new_mip);
            profiling::scope!(&_label);

            encoder.copy_texture_to_texture(
                ImageCopyTexture {
                    texture: old_texture,
                    mip_level: old_mip,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                ImageCopyTexture {
                    texture: &tex,
                    mip_level: new_mip,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                old_texture_desc.mip_level_size(old_mip).unwrap(),
            );
        }
        self.instructions.push(
            InstructionKind::AddTexture {
                handle: handle.clone(),
                texture: tex,
                desc,
                view,
                buffer: Some(encoder.finish()),
                cube: false,
            },
            *Location::caller(),
        );
        handle
    }

    /// Adds a Cube texture to the renderer. This can be used as a cube
    /// environment map by a render routine.
    ///
    /// The handle will keep the texture alive.
    #[track_caller]
    pub fn add_texture_cube(&self, texture: Texture) -> TextureHandle {
        profiling::scope!("Add Texture Cube");

        Self::validation_texture_format(texture.format);

        let handle = TextureManager::allocate(&self.current_ident);
        let size = Extent3d {
            width: texture.size.x,
            height: texture.size.y,
            depth_or_array_layers: 6,
        };

        let mip_level_count = match texture.mip_count {
            MipmapCount::Specific(v) => v.get(),
            MipmapCount::Maximum => size.max_mips(),
        };

        let desc = TextureDescriptor {
            label: None,
            size,
            mip_level_count,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: texture.format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        };

        let tex = self.device.create_texture_with_data(&self.queue, &desc, &texture.data);

        let view = tex.create_view(&TextureViewDescriptor {
            dimension: Some(TextureViewDimension::Cube),
            ..TextureViewDescriptor::default()
        });
        self.instructions.push(
            InstructionKind::AddTexture {
                handle: handle.clone(),
                texture: tex,
                desc,
                view,
                buffer: None,
                cube: true,
            },
            *Location::caller(),
        );
        handle
    }

    fn validation_texture_format(format: TextureFormat) {
        let sample_type = format.describe().sample_type;
        if let TextureSampleType::Float { filterable } = sample_type {
            if !filterable {
                panic!(
                    "Textures formats must allow filtering with a linear filter. {:?} has sample type {:?} which does not.",
                    format, sample_type
                )
            }
        } else {
            panic!(
                "Textures formats must be sample-able as floating point. {:?} has sample type {:?}.",
                format, sample_type
            )
        }
    }

    /// Adds a material to the renderer. This can be used in an [`Object`].
    ///
    /// The handle will keep the material alive. All objects created with this
    /// material will also keep this material alive.
    ///
    /// The material will keep the inside textures alive.
    #[track_caller]
    pub fn add_material<M: Material>(&self, material: M) -> MaterialHandle {
        let handle = MaterialManager::allocate(&self.current_ident);
        self.instructions.push(
            InstructionKind::AddMaterial {
                handle: handle.clone(),
                fill_invoke: Box::new(move |material_manager, device, profile, d2_manager, mat_handle| {
                    material_manager.fill(device, profile, d2_manager, mat_handle, material)
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
                handle: handle.clone(),
                change_invoke: Box::new(
                    move |material_manager, device, profile, d2_manager, object_manager, mat_handle| {
                        material_manager.update(device, profile, d2_manager, object_manager, mat_handle, material)
                    },
                ),
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
    pub fn add_object(&self, object: Object) -> ObjectHandle {
        let handle = ObjectManager::allocate(&self.current_ident);
        self.instructions.push(
            InstructionKind::AddObject {
                handle: handle.clone(),
                object,
            },
            *Location::caller(),
        );
        handle
    }

    /// Duplicates an existing object in the renderer, returning the new
    /// object's handle. Any changes specified in the `change` struct will be
    /// applied to the duplicated object, and the same mesh, material and
    /// transform as the original object will be used otherwise.
    #[track_caller]
    pub fn duplicate_object(&self, object_handle: &ObjectHandle, change: ObjectChange) -> ObjectHandle {
        let dst_handle = ObjectManager::allocate(&self.current_ident);
        self.instructions.push(
            InstructionKind::DuplicateObject {
                src_handle: object_handle.clone(),
                dst_handle: dst_handle.clone(),
                change,
            },
            *Location::caller(),
        );
        dst_handle
    }

    /// Move the given object to a new transform location.
    #[track_caller]
    pub fn set_object_transform(&self, handle: &ObjectHandle, transform: Mat4) {
        self.instructions.push(
            InstructionKind::SetObjectTransform {
                handle: handle.get_raw(),
                transform,
            },
            *Location::caller(),
        );
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
            InstructionKind::SetSkeletonJointDeltas {
                handle: handle.get_raw(),
                joint_matrices,
            },
            *Location::caller(),
        )
    }

    /// Add a sun-like light into the world.
    ///
    /// The handle will keep the light alive.
    #[track_caller]
    pub fn add_directional_light(&self, light: DirectionalLight) -> DirectionalLightHandle {
        let handle = DirectionalLightManager::allocate(&self.current_ident);

        self.instructions.push(
            InstructionKind::AddDirectionalLight {
                handle: handle.clone(),
                light,
            },
            *Location::caller(),
        );

        handle
    }

    /// Updates the settings for given directional light.
    #[track_caller]
    pub fn update_directional_light(&self, handle: &DirectionalLightHandle, change: DirectionalLightChange) {
        self.instructions.push(
            InstructionKind::ChangeDirectionalLight {
                handle: handle.get_raw(),
                change,
            },
            *Location::caller(),
        )
    }

    /// Sets the aspect ratio of the camera. This should correspond with the
    /// aspect ratio of the user.
    #[track_caller]
    pub fn set_aspect_ratio(&self, ratio: f32) {
        self.instructions
            .push(InstructionKind::SetAspectRatio { ratio }, *Location::caller())
    }

    /// Sets the position, pov, or projection mode of the camera.
    #[track_caller]
    pub fn set_camera_data(&self, data: Camera) {
        self.instructions
            .push(InstructionKind::SetCameraData { data }, *Location::caller())
    }

    /// Render a frame of the scene onto the given output, using the given
    /// RenderRoutine.
    ///
    /// The RendererStatistics may not be the results from this frame, but might
    /// be the results from multiple frames ago.
    pub fn ready(&self) -> (Vec<CommandBuffer>, ReadyData) {
        ready::ready(self)
    }
}
