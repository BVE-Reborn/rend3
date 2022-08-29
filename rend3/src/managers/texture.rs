use crate::{profile::ProfileData, types::TextureHandle, RendererProfile};
use rend3_types::{RawTextureHandle, TextureFormat, TextureUsages};
use std::{
    num::NonZeroU32,
    sync::{
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
    pub view: TextureView,
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

    data: Vec<Option<InternalTexture>>,

    dimension: TextureViewDimension,
}
impl TextureManager {
    pub fn new(device: &Device, profile: RendererProfile, texture_limit: u32, dimension: TextureViewDimension) -> Self {
        profiling::scope!("TextureManager::new");

        let null_view = create_null_tex_view(device, dimension);

        let max_textures = (texture_limit / BGL_DIVISOR).min(MAX_TEXTURE_COUNT);

        let mut data = Vec::with_capacity(TEXTURE_PREALLOCATION);
        data.resize_with(TEXTURE_PREALLOCATION, || None);

        let layout = profile.into_data(|| (), || create_bind_group_layout(device, max_textures, dimension));
        let group = profile.into_data(
            || (),
            || create_bind_group(device, layout.as_gpu(), &null_view, &data, dimension),
        );

        Self {
            layout,
            group,
            group_dirty: profile.into_data(|| (), || false),
            null_view,
            data,
            dimension,
        }
    }

    pub fn add(
        &mut self,
        handle: &TextureHandle,
        desc: TextureDescriptor<'static>,
        texture: Texture,
        view: TextureView,
    ) {
        self.group_dirty = self.group_dirty.map_gpu(|_| true);

        if handle.idx >= self.data.len() {
            self.data.resize_with(handle.idx + 1, || None);
        }
        self.data[handle.idx] = Some(InternalTexture { texture, view, desc });
    }

    pub fn remove(
        &mut self,
        handle: RawTextureHandle,
    ) {
        self.group_dirty = self.group_dirty.map_gpu(|_| true);
        
        self.data[handle.idx] = None;
    }

    pub fn ready(&mut self, device: &Device) -> TextureManagerReadyOutput {
        profiling::scope!("TextureManager::ready");

        if let ProfileData::Gpu(group_dirty) = self.group_dirty {
            profiling::scope!("Update GPU Texture Arrays");

            if group_dirty {
                *self.group.as_gpu_mut() = create_bind_group(
                    device,
                    self.layout.as_gpu(),
                    &self.null_view,
                    &self.data,
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
        self.data[handle.idx].as_ref().unwrap()
    }

    pub fn get_view(&self, handle: RawTextureHandle) -> &TextureView {
        &self.data[handle.idx].as_ref().unwrap().view
    }

    pub fn get_null_view(&self) -> &TextureView {
        &self.null_view
    }

    pub fn gpu_bgl(&self) -> &BindGroupLayout {
        self.layout.as_gpu()
    }

    pub fn translation_fn(&self) -> impl Fn(RawTextureHandle) -> NonZeroU32 + Copy + '_ {
        move |v: RawTextureHandle| NonZeroU32::new(v.idx as u32 + 1).unwrap()
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
    data: &[Option<InternalTexture>],
    dimension: TextureViewDimension,
) -> Arc<BindGroup> {
    let count = data.len();
    let mut view_array = Vec::with_capacity(count);
    view_array.extend(data.iter().map(|tex| match *tex {
        Some(t) => &t.view,
        None => null_view,
    }));
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
