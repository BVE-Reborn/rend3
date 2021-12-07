use crate::{
    format_sso,
    instruction::{InstructionKind, InstructionStreamPair},
    managers::{
        CameraManager, DirectionalLightManager, InternalTexture, MaterialManager, MeshManager, ObjectManager,
        TextureManager,
    },
    types::{
        Camera, DirectionalLight, DirectionalLightChange, DirectionalLightHandle, MaterialHandle, Mesh, MeshHandle,
        Object, ObjectHandle, Texture, TextureHandle,
    },
    util::{graph_texture_store::GraphTextureStore, mipmap::MipmapGenerator},
    ExtendedAdapterInfo, InstanceAdapterDevice, ReadyData, RendererInitializationError, RendererMode,
};
use glam::Mat4;
use parking_lot::{Mutex, RwLock};
use rend3_types::{Material, MipmapCount, MipmapSource, TextureFormat, TextureFromTexture, TextureUsages};
use std::{num::NonZeroU32, panic::Location, sync::Arc};
use wgpu::{
    util::DeviceExt, CommandBuffer, CommandEncoderDescriptor, Device, Extent3d, ImageCopyTexture, ImageDataLayout,
    Origin3d, Queue, TextureAspect, TextureDescriptor, TextureDimension, TextureSampleType, TextureViewDescriptor,
    TextureViewDimension,
};
use wgpu_profiler::GpuProfiler;

pub mod error;
mod ready;
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

    /// Tool which generates mipmaps from a texture.
    pub mipmap_generator: MipmapGenerator,

    /// Stores gpu timing and debug scopes.
    pub profiler: Mutex<GpuProfiler>,

    /// Stores a cache of render targets between graph invocations.
    pub(crate) graph_texture_store: Mutex<GraphTextureStore>,
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
    #[track_caller]
    pub fn add_mesh(&self, mesh: Mesh) -> MeshHandle {
        let handle = self.mesh_manager.read().allocate();

        self.instructions.push(
            InstructionKind::AddMesh {
                handle: handle.clone(),
                mesh,
            },
            *Location::caller(),
        );

        handle
    }

    /// Add a 2D texture to the renderer. This can be used in a [`Material`].
    ///
    /// The handle will keep the texture alive. All materials created with this texture will also keep the texture alive.
    #[track_caller]
    pub fn add_texture_2d(&self, texture: Texture) -> TextureHandle {
        profiling::scope!("Add Texture 2D");

        Self::validation_texture_format(texture.format);

        let handle = self.d2_texture_manager.read().allocate();
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
                    .generate_mipmaps(&self.device, &self.profiler, &mut encoder, &tex, &desc);

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

    /// Add a 2D texture to the renderer by copying a set of mipmaps from an existing texture. This new can be used in a [`Material`].
    ///
    /// The handle will keep the texture alive. All materials created with this texture will also keep the texture alive.
    #[track_caller]
    pub fn add_texture_2d_from_texture(&self, texture: TextureFromTexture) -> TextureHandle {
        profiling::scope!("Add Texture 2D From Texture");

        let mut encoder = self.device.create_command_encoder(&CommandEncoderDescriptor::default());

        let d2_manager = self.d2_texture_manager.read();
        let handle = d2_manager.allocate();
        // self.profiler
        //     .lock()
        //     .begin_scope("Add Texture 2D From Texture", &mut encoder, &self.device);

        let InternalTexture {
            texture: old_texture,
            desc: old_texture_desc,
        } = d2_manager.get_internal(texture.src.get_raw());

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
            // self.profiler.lock().begin_scope(&label, &mut encoder, &self.device);

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

            // self.profiler.lock().end_scope(&mut encoder);
        }
        // self.profiler.lock().end_scope(&mut encoder);
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

    /// Adds a Cube texture to the renderer. This can be used as a cube environment map by a render routine.
    ///
    /// The handle will keep the texture alive.
    #[track_caller]
    pub fn add_texture_cube(&self, texture: Texture) -> TextureHandle {
        profiling::scope!("Add Texture Cube");

        Self::validation_texture_format(texture.format);

        let handle = self.d2c_texture_manager.read().allocate();
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
    /// The handle will keep the material alive. All objects created with this material will also keep this material alive.
    ///
    /// The material will keep the inside textures alive.
    #[track_caller]
    pub fn add_material<M: Material>(&self, material: M) -> MaterialHandle {
        let handle = self.material_manager.read().allocate();
        self.instructions.push(
            InstructionKind::AddMaterial {
                handle: handle.clone(),
                fill_invoke: Box::new(move |material_manager, device, mode, d2_manager, mat_handle| {
                    material_manager.fill(device, mode, d2_manager, mat_handle, material)
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
                    move |material_manager, device, mode, d2_manager, object_manager, mat_handle| {
                        material_manager.update(device, mode, d2_manager, object_manager, mat_handle, material)
                    },
                ),
            },
            *Location::caller(),
        )
    }

    /// Adds an object to the renderer. This will create a visible object using the given mesh and materal.
    ///
    /// The handle will keep the material alive.
    ///
    /// The object will keep all materials, textures, and meshes alive.
    #[track_caller]
    pub fn add_object(&self, object: Object) -> ObjectHandle {
        let handle = self.object_manager.read().allocate();
        self.instructions.push(
            InstructionKind::AddObject {
                handle: handle.clone(),
                object,
            },
            *Location::caller(),
        );
        handle
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

    /// Add a sun-like light into the world.
    ///
    /// The handle will keep the light alive.
    #[track_caller]
    pub fn add_directional_light(&self, light: DirectionalLight) -> DirectionalLightHandle {
        let handle = self.directional_light_manager.read().allocate();

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

    /// Sets the aspect ratio of the camera. This should correspond with the aspect ratio of the user.
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

    /// Render a frame of the scene onto the given output, using the given RenderRoutine.
    ///
    /// The RendererStatistics may not be the results from this frame, but might be the results from multiple frames ago.
    pub fn ready(&self) -> (Vec<CommandBuffer>, ReadyData) {
        ready::ready(self)
    }
}
