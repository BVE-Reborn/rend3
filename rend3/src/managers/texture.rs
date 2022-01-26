use crate::{profile::ProfileData, types::TextureHandle, util::registry::ResourceRegistry, RendererProfile};
use rend3_types::{RawTextureHandle, TextureFormat, TextureUsages};
use std::{
    num::NonZeroU32,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, Device, Extent3d, ShaderStages, Texture, TextureDescriptor, TextureDimension,
    TextureSampleType, TextureView, TextureViewDescriptor, TextureViewDimension,
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
    pub desc: TextureDescriptor<'static>,
}

/// Preallocation count of texture view array
const TEXTURE_PREALLOCATION: usize = 1024;
/// What we divide the texture limit by to get the count supplied in the BGL.
const BGL_DIVISOR: u32 = 4;

/// Manages textures and associated bindless bind groups
pub struct TextureManager {
    layout: ProfileData<(), Arc<BindGroupLayout>>,
    group: ProfileData<(), Arc<BindGroup>>,
    group_dirty: ProfileData<(), bool>,

    null_view: TextureView,

    views: Vec<TextureView>,
    registry: ResourceRegistry<InternalTexture, rend3_types::Texture>,

    dimension: TextureViewDimension,
}
impl TextureManager {
    pub fn new(device: &Device, profile: RendererProfile, texture_limit: u32, dimension: TextureViewDimension) -> Self {
        profiling::scope!("TextureManager::new");

        let views = Vec::with_capacity(TEXTURE_PREALLOCATION);

        let null_view = create_null_tex_view(device, dimension);

        let max_textures = (texture_limit / BGL_DIVISOR).min(MAX_TEXTURE_COUNT);

        let layout = profile.into_data(|| (), || create_bind_group_layout(device, max_textures, dimension));
        let group = profile.into_data(
            || (),
            || create_bind_group(device, layout.as_gpu(), &null_view, views.iter(), dimension),
        );

        let registry = ResourceRegistry::new();

        Self {
            layout,
            group,
            group_dirty: profile.into_data(|| (), || false),
            null_view,
            views,
            registry,
            dimension,
        }
    }

    pub fn allocate(counter: &AtomicUsize) -> TextureHandle {
        let idx = counter.fetch_add(1, Ordering::Relaxed);

        TextureHandle::new(idx)
    }

    pub fn fill(
        &mut self,
        handle: &TextureHandle,
        desc: TextureDescriptor<'static>,
        texture: Texture,
        view: TextureView,
    ) {
        self.group_dirty = self.group_dirty.map_gpu(|_| true);

        self.registry.insert(handle, InternalTexture { texture, desc });

        self.views.push(view);
    }

    pub fn internal_index(&self, handle: RawTextureHandle) -> usize {
        self.registry.get_index_of(handle)
    }

    pub fn ready(&mut self, device: &Device) -> TextureManagerReadyOutput {
        profiling::scope!("TextureManager::ready");

        let views = &mut self.views;
        self.registry.remove_all_dead(|_, index, _| {
            // Do the same swap remove move as the registry did
            views.swap_remove(index);
        });

        if let ProfileData::Gpu(group_dirty) = self.group_dirty {
            profiling::scope!("Update GPU Texture Arrays");

            if group_dirty {
                *self.group.as_gpu_mut() = create_bind_group(
                    device,
                    self.layout.as_gpu(),
                    &self.null_view,
                    self.views.iter(),
                    self.dimension,
                );
                *self.group_dirty.as_gpu_mut() = false;
            }

            TextureManagerReadyOutput {
                bg: self.group.as_ref().map(|_| (), Arc::clone),
            }
        } else {
            TextureManagerReadyOutput {
                bg: ProfileData::Cpu(()),
            }
        }
    }

    pub fn get_internal(&self, handle: RawTextureHandle) -> &InternalTexture {
        self.registry.get(handle)
    }

    pub fn get_view_from_index(&self, idx: NonZeroU32) -> &TextureView {
        &self.views[(idx.get() - 1) as usize]
    }

    pub fn get_view(&self, handle: RawTextureHandle) -> &TextureView {
        &self.views[self.registry.get_index_of(handle)]
    }

    pub fn get_null_view(&self) -> &TextureView {
        &self.null_view
    }

    pub fn gpu_bgl(&self) -> &BindGroupLayout {
        self.layout.as_gpu()
    }

    pub fn translation_fn(&self) -> impl Fn(&TextureHandle) -> NonZeroU32 + Copy + '_ {
        move |v: &TextureHandle| NonZeroU32::new(self.internal_index(v.get_raw()) as u32 + 1).unwrap()
    }
}

/// Output of readying up a [`TextureManager`].
#[derive(Clone)]
pub struct TextureManagerReadyOutput {
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

fn create_bind_group<'a>(
    device: &Device,
    layout: &BindGroupLayout,
    null_view: &'a TextureView,
    views: impl ExactSizeIterator<Item = &'a TextureView>,
    dimension: TextureViewDimension,
) -> Arc<BindGroup> {
    let mut view_array = Vec::with_capacity(views.len().max(1));
    let count = views.len();
    if count == 0 {
        view_array.push(null_view);
    } else {
        view_array.extend(views);
    }
    Arc::new(device.create_bind_group(&BindGroupDescriptor {
        label: Some(&*format!("{:?} texture bg count {}", dimension, count)),
        layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureViewArray(&view_array),
        }],
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
        })
        .create_view(&TextureViewDescriptor {
            dimension: Some(dimension),
            ..TextureViewDescriptor::default()
        })
}
