use std::{marker::PhantomData, num::NonZeroU32, sync::Arc};

use rend3_types::{MipmapCount, MipmapSource, RawResourceHandle, TextureFormat, TextureFromTexture, TextureUsages};
use thiserror::Error;
use wgpu::{
    util::DeviceExt, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, CommandBuffer, CommandEncoder, CommandEncoderDescriptor,
    Device, Extent3d, Features, TexelCopyTextureInfo, TexelCopyBufferLayout, Origin3d, ShaderStages, Texture, TextureAspect,
    TextureDescriptor, TextureDimension, TextureSampleType, TextureView, TextureViewDescriptor, TextureViewDimension,
};

use crate::{
    profile::ProfileData,
    util::{error_scope::AllocationErrorScope, math::round_up, mipmap::MipmapGenerationError},
    Renderer, RendererProfile,
};

/// When using the GpuDriven profile, we start the 2D texture manager with a bind group with
/// this many textures.
pub const STARTING_2D_TEXTURES: usize = 1 << 8;
/// When using the GpuDriven profile, we start the Cubemap texture manager with a bind group
/// with this many textures.
pub const STARTING_CUBE_TEXTURES: usize = 1 << 3;
/// Largest amount of supported textures per type
pub const MAX_TEXTURE_COUNT: u32 = 1 << 17;

/// Internal representation of a Texture.
pub struct InternalTexture {
    pub texture: Texture,
    pub view: TextureView,
    pub desc: TextureDescriptor<'static>,
}

#[derive(Debug, Error)]
pub enum TextureCreationError {
    #[error("Failed to allocate texture")]
    TextureAllocationFailed(#[source] wgpu::Error),
    #[error("Failed to write to texture")]
    WriteTextureFailed(#[source] wgpu::Error),
    #[error("Failed to create texture view")]
    TextureViewCreationFailed(#[source] wgpu::Error),
    #[error("Textures formats must allow filtering with a linear filter. {format:?} has sample type {sample_type:?} which does not.")]
    TextureFormatNotFilterable { format: TextureFormat, sample_type: TextureSampleType },
    #[error("Textures formats must be sample-able as floating point. {format:?} has sample type {sample_type:?}.")]
    TextureFormatNotFloat { format: TextureFormat, sample_type: Option<TextureSampleType> },
    #[error("Mipmap creation failed")]
    MipmapCreationFailed(#[from] MipmapGenerationError),
}

unsafe impl Send for TextureCreationError {}
unsafe impl Sync for TextureCreationError {}

/// Preallocation count of texture view array
const TEXTURE_PREALLOCATION: usize = 1024;
/// What we divide the texture limit by to get the count supplied in the BGL.
const BGL_DIVISOR: u32 = 4;

/// Manages textures and associated bindless bind groups
pub struct TextureManager<T> {
    layout: ProfileData<(), Arc<BindGroupLayout>>,
    group: ProfileData<(), Arc<BindGroup>>,
    group_dirty: ProfileData<(), bool>,

    null_view: TextureView,

    data: Vec<Option<InternalTexture>>,

    dimension: TextureViewDimension,

    _phantom: PhantomData<T>,
}
impl<T: 'static> TextureManager<T> {
    pub fn new(device: &Device, profile: RendererProfile, texture_limit: u32, dimension: TextureViewDimension) -> Self {
        profiling::scope!("TextureManager::new");

        let null_view = create_null_tex_view(device, dimension);

        let max_textures = (texture_limit / BGL_DIVISOR).min(MAX_TEXTURE_COUNT);

        let mut data = Vec::with_capacity(TEXTURE_PREALLOCATION);
        data.resize_with(TEXTURE_PREALLOCATION, || None);

        let layout = profile.into_data(|| (), || create_bind_group_layout(device, max_textures, dimension));
        let group =
            profile.into_data(|| (), || create_bind_group(device, layout.as_gpu(), &null_view, &data, dimension));

        Self {
            layout,
            group,
            group_dirty: profile.into_data(|| (), || false),
            null_view,
            data,
            dimension,
            _phantom: PhantomData,
        }
    }

    pub fn add(
        renderer: &Renderer,
        texture: crate::types::Texture,
        cube: bool,
    ) -> Result<(Option<CommandBuffer>, InternalTexture), TextureCreationError> {
        validate_texture_format(texture.format, renderer.features)?;

        let (block_x, block_y) = texture.format.block_dimensions();
        let size = Extent3d {
            width: round_up(texture.size.x, block_x),
            height: round_up(texture.size.y, block_y),
            depth_or_array_layers: match cube {
                true => 6,
                false => 1,
            },
        };

        let mip_level_count = match texture.mip_count {
            MipmapCount::Specific(v) => v.get(),
            MipmapCount::Maximum => size.max_mips(match cube {
                true => wgpu::TextureDimension::D3,
                false => wgpu::TextureDimension::D2,
            }),
        };

        let desc = TextureDescriptor {
            label: None,
            size,
            mip_level_count,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: texture.format,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::COPY_DST,
            view_formats: &[],
        };

        let (buffer, tex) = match texture.mip_source {
            MipmapSource::Uploaded => {
                let scope = AllocationErrorScope::new(&renderer.device);
                let texture = renderer.device.create_texture_with_data(
                    &renderer.queue,
                    &desc,
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &texture.data,
                );
                scope.end().map_err(TextureCreationError::TextureAllocationFailed)?;
                (None, texture)
            }
            MipmapSource::Generated => {
                assert!(!cube, "Cannot generate mipmaps from cubemaps currently");

                let desc = TextureDescriptor { usage: desc.usage | TextureUsages::RENDER_ATTACHMENT, ..desc };
                let scope = AllocationErrorScope::new(&renderer.device);
                let tex = renderer.device.create_texture(&desc);
                scope.end().map_err(TextureCreationError::TextureAllocationFailed)?;

                let (block_width, _) = texture.format.block_dimensions();
                let block_size = texture.format.block_copy_size(None).unwrap();

                let scope = AllocationErrorScope::new(&renderer.device);
                // write first level
                renderer.queue.write_texture(
                    TexelCopyTextureInfo {
                        texture: &tex,
                        mip_level: 0,
                        origin: Origin3d::ZERO,
                        aspect: TextureAspect::All,
                    },
                    &texture.data,
                    TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(block_size * (size.width / block_width)),
                        rows_per_image: None,
                    },
                    size,
                );
                scope.end().map_err(TextureCreationError::WriteTextureFailed)?;

                let mut encoder = renderer.device.create_command_encoder(&CommandEncoderDescriptor::default());

                // generate mipmaps
                renderer.mipmap_generator.generate_mipmaps(&renderer.device, &mut encoder, &tex, &desc)?;

                (Some(encoder.finish()), tex)
            }
        };

        let scope = AllocationErrorScope::new(&renderer.device);
        let view = tex.create_view(&TextureViewDescriptor {
            dimension: match cube {
                true => Some(TextureViewDimension::Cube),
                false => Some(TextureViewDimension::D2),
            },
            ..Default::default()
        });
        scope.end().map_err(TextureCreationError::TextureViewCreationFailed)?;

        Ok((buffer, InternalTexture { texture: tex, view, desc }))
    }

    pub fn fill_from_texture(
        &mut self,
        device: &Device,
        encoder: &mut CommandEncoder,
        dst_handle: RawResourceHandle<T>,
        texture: TextureFromTexture,
    ) {
        let InternalTexture { texture: old_texture, desc: old_texture_desc, .. } =
            self.data[texture.src.idx].as_ref().unwrap();

        let new_size = old_texture_desc.mip_level_size(texture.start_mip).unwrap();

        let mip_level_count =
            texture.mip_count.map_or_else(|| old_texture_desc.mip_level_count - texture.start_mip, |c| c.get());

        let desc = TextureDescriptor { size: new_size, mip_level_count, ..old_texture_desc.clone() };

        let tex = device.create_texture(&desc);

        let view = tex.create_view(&TextureViewDescriptor::default());

        for new_mip in 0..mip_level_count {
            let old_mip = new_mip + texture.start_mip;

            profiling::scope!("mip level generation");

            encoder.copy_texture_to_texture(
                TexelCopyTextureInfo {
                    texture: old_texture,
                    mip_level: old_mip,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                TexelCopyTextureInfo {
                    texture: &tex,
                    mip_level: new_mip,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                old_texture_desc.mip_level_size(old_mip).unwrap(),
            );
        }

        self.fill(dst_handle, InternalTexture { texture: tex, view, desc })
    }

    pub fn fill(&mut self, handle: RawResourceHandle<T>, internal_texture: InternalTexture) {
        self.group_dirty = self.group_dirty.map_gpu(|_| true);

        if handle.idx >= self.data.len() {
            self.data.resize_with(handle.idx + 1, || None);
        }
        self.data[handle.idx] = Some(internal_texture);
    }

    pub fn remove(&mut self, handle: &RawResourceHandle<T>) {
        self.group_dirty = self.group_dirty.map_gpu(|_| true);

        self.data[handle.idx] = None;
    }

    pub fn evaluate(&mut self, device: &Device) -> TextureManagerEvaluateOutput {
        profiling::scope!("TextureManager::evaluate");

        if let ProfileData::Gpu(group_dirty) = self.group_dirty {
            profiling::scope!("Update GPU Texture Arrays");

            if group_dirty {
                *self.group.as_gpu_mut() =
                    create_bind_group(device, self.layout.as_gpu(), &self.null_view, &self.data, self.dimension);
                *self.group_dirty.as_gpu_mut() = false;
            }

            TextureManagerEvaluateOutput { bg: self.group.as_ref().map(|_| (), Arc::clone) }
        } else {
            TextureManagerEvaluateOutput { bg: ProfileData::Cpu(()) }
        }
    }

    pub fn get_internal(&self, handle: RawResourceHandle<T>) -> &InternalTexture {
        self.data[handle.idx].as_ref().unwrap()
    }

    pub fn get_view(&self, handle: &RawResourceHandle<T>) -> &TextureView {
        &self.data[handle.idx].as_ref().unwrap().view
    }

    pub fn get_null_view(&self) -> &TextureView {
        &self.null_view
    }

    pub fn gpu_bgl(&self) -> &BindGroupLayout {
        self.layout.as_gpu()
    }

    pub fn translation_fn(&self) -> impl Fn(RawResourceHandle<T>) -> NonZeroU32 + Copy + '_ {
        move |v: RawResourceHandle<T>| NonZeroU32::new(v.idx as u32 + 1).unwrap()
    }
}

/// Output of calling [`TextureManager::evaluate`].
#[derive(Clone)]
pub struct TextureManagerEvaluateOutput {
    pub bg: ProfileData<(), Arc<BindGroup>>,
}

fn create_bind_group_layout(device: &Device, count: u32, view_dimension: TextureViewDimension) -> Arc<BindGroupLayout> {
    Arc::new(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some(&*format!("{:?} texture bgl", view_dimension)),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStages::FRAGMENT,
            ty: BindingType::Texture {
                view_dimension,
                sample_type: TextureSampleType::Float { filterable: true },
                multisampled: false,
            },
            count: NonZeroU32::new(count),
        }],
    }))
}

fn create_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    null_view: &TextureView,
    data: &[Option<InternalTexture>],
    dimension: TextureViewDimension,
) -> Arc<BindGroup> {
    let count = data.len();
    let mut view_array = Vec::with_capacity(count);
    view_array.extend(data.iter().map(|tex| match tex {
        Some(t) => &t.view,
        None => null_view,
    }));
    Arc::new(device.create_bind_group(&BindGroupDescriptor {
        label: Some(&*format!("{:?} texture bg count {}", dimension, count)),
        layout,
        entries: &[BindGroupEntry { binding: 0, resource: BindingResource::TextureViewArray(&view_array) }],
    }))
}

fn create_null_tex_view(device: &Device, dimension: TextureViewDimension) -> TextureView {
    device
        .create_texture(&TextureDescriptor {
            label: Some(&*format!("null {:?} texture", dimension)),
            size: Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: match dimension {
                    TextureViewDimension::Cube | TextureViewDimension::CubeArray => 6,
                    _ => 1,
                },
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: match dimension {
                TextureViewDimension::D1 => TextureDimension::D1,
                TextureViewDimension::D2
                | TextureViewDimension::D2Array
                | TextureViewDimension::Cube
                | TextureViewDimension::CubeArray => TextureDimension::D2,
                TextureViewDimension::D3 => TextureDimension::D3,
            },
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&TextureViewDescriptor { dimension: Some(dimension), ..TextureViewDescriptor::default() })
}

fn validate_texture_format(format: TextureFormat, features: Features) -> Result<(), TextureCreationError> {
    let sample_type = format.sample_type(None, Some(features));
    match sample_type {
        Some(TextureSampleType::Float { filterable: true }) => Ok(()),
        Some(TextureSampleType::Float { filterable: false }) => {
            Err(TextureCreationError::TextureFormatNotFilterable { format, sample_type: sample_type.unwrap() })
        }
        _ => Err(TextureCreationError::TextureFormatNotFloat { format, sample_type }),
    }
}
