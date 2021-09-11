use crate::{mode::ModeData, types::TextureHandle, util::registry::ResourceRegistry, RendererMode};
use rend3_types::RawTextureHandle;
use std::{mem, num::NonZeroU32, sync::Arc};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, Device, Extent3d, ShaderStages, Texture, TextureDescriptor, TextureDimension,
    TextureFormat, TextureSampleType, TextureUsages, TextureView, TextureViewDescriptor, TextureViewDimension,
};

/// When using GPU mode, we start the 2D texture manager with a bind group with this many textures.
// TODO: Intel's very low limit on windows will cause issues for users, so force them off gpu mode by forcing this number larger than their limits until we can solve this. See https://github.com/gfx-rs/wgpu/issues/1111
pub const STARTING_2D_TEXTURES: usize = 1 << 8;
/// When using GPU mode, we start the Cubemap texture manager with a bind group with this many textures.
pub const STARTING_CUBE_TEXTURES: usize = 1 << 3;

/// Internal representation of a Texture.
pub struct InternalTexture {
    pub texture: Texture,
    pub desc: TextureDescriptor<'static>,
}

/// Manages textures and associated bindless bind groups
pub struct TextureManager {
    layout: ModeData<(), Arc<BindGroupLayout>>,
    layout_dirty: ModeData<(), bool>,

    group: ModeData<(), Arc<BindGroup>>,
    group_dirty: ModeData<(), bool>,

    null_tex_man: NullTextureManager,

    views: Vec<TextureView>,
    registry: ResourceRegistry<InternalTexture, rend3_types::Texture>,

    dimension: TextureViewDimension,
}
impl TextureManager {
    pub fn new(device: &Device, mode: RendererMode, starting_textures: usize, dimension: TextureViewDimension) -> Self {
        let mut null_tex_man = NullTextureManager::new(device, dimension);

        let view_count = starting_textures;

        let mut views = Vec::with_capacity(view_count);
        fill_to_size(&mut null_tex_man, &mut views, dimension, view_count);

        let layout = mode.into_data(|| (), || create_bind_group_layout(device, view_count as u32, dimension));
        let group = mode.into_data(
            || (),
            || create_bind_group(device, layout.as_gpu(), &views.iter().collect::<Vec<_>>(), dimension),
        );

        let registry = ResourceRegistry::new();

        Self {
            layout,
            layout_dirty: mode.into_data(|| (), || false),
            group,
            group_dirty: mode.into_data(|| (), || false),
            null_tex_man,
            views,
            registry,
            dimension,
        }
    }

    pub fn allocate(&self) -> TextureHandle {
        self.registry.allocate()
    }

    pub fn fill(
        &mut self,
        handle: &TextureHandle,
        desc: TextureDescriptor<'static>,
        texture: Texture,
        view: TextureView,
    ) {
        self.group_dirty = self.group_dirty.map_gpu(|_| true);

        let index = self.registry.insert(handle, InternalTexture { texture, desc });

        if index >= self.views.len() {
            self.layout_dirty = self.layout_dirty.map_gpu(|_| true);

            let new_size = self.views.len() * 2;
            fill_to_size(&mut self.null_tex_man, &mut self.views, self.dimension, new_size);
        }

        let old_null = mem::replace(&mut self.views[index], view);

        self.null_tex_man.put(old_null);
    }

    pub fn internal_index(&self, handle: RawTextureHandle) -> usize {
        self.registry.get_index_of(handle)
    }

    pub fn ready(&mut self, device: &Device) -> TextureManagerReadyOutput {
        profiling::scope!("D2 Texture Manager Ready");
        let dimension = self.dimension;
        let null_tex_man = &mut self.null_tex_man;
        let views = &mut self.views;
        self.registry.remove_all_dead(|registry, index, _| {
            let active_count = registry.count();

            // Do the same swap remove move as the registry did
            if active_count > 1 {
                views.swap(index, active_count);
            }
            // Overwrite the last item with the null tex
            views[active_count] = null_tex_man.get(dimension);
        });

        if let ModeData::GPU(_) = self.layout_dirty {
            profiling::scope!("Update GPU Texture Arrays");
            let layout_dirty = self.layout_dirty;

            if self.layout_dirty.into_gpu() {
                *self.layout.as_gpu_mut() = create_bind_group_layout(device, self.views.len() as u32, self.dimension);
                *self.layout_dirty.as_gpu_mut() = false;
            }

            if self.group_dirty.into_gpu() {
                *self.group.as_gpu_mut() = create_bind_group(
                    device,
                    self.layout.as_gpu(),
                    &self.views.iter().collect::<Vec<_>>(),
                    self.dimension,
                );
                *self.group_dirty.as_gpu_mut() = false;
            }

            TextureManagerReadyOutput {
                bg: self.group.as_ref().map(|_| (), Arc::clone),
                dirty: layout_dirty,
            }
        } else {
            TextureManagerReadyOutput {
                bg: ModeData::CPU(()),
                dirty: ModeData::CPU(()),
            }
        }
    }

    pub fn get_internal(&self, handle: RawTextureHandle) -> &InternalTexture {
        self.registry.get(handle)
    }

    pub fn get_view(&self, handle: RawTextureHandle) -> &TextureView {
        &self.views[self.registry.get_index_of(handle)]
    }

    pub fn ensure_null_view(&mut self) {
        self.null_tex_man.ensure_at_least_one(self.dimension)
    }

    pub fn get_null_view(&self) -> &TextureView {
        self.null_tex_man.get_ref()
    }

    pub fn gpu_bgl(&self) -> &BindGroupLayout {
        self.layout.as_gpu()
    }

    pub fn translation_fn(&self) -> impl Fn(&TextureHandle) -> NonZeroU32 + Copy + '_ {
        move |v: &TextureHandle| NonZeroU32::new(self.internal_index(v.get_raw()) as u32 + 1).unwrap()
    }
}

/// Output of readying up a [`TextureManager`].
pub struct TextureManagerReadyOutput {
    // TODO(0.10) https://github.com/gfx-rs/wgpu/issues/
    pub bg: ModeData<(), Arc<BindGroup>>,
    /// The BindGroupLayout has changed, and we need to recreate used pipelines.
    pub dirty: ModeData<(), bool>,
}

fn fill_to_size(
    null_tex_man: &mut NullTextureManager,
    views: &mut Vec<TextureView>,
    dimension: TextureViewDimension,
    size: usize,
) {
    let to_add = size.saturating_sub(views.len());

    for _ in 0..to_add {
        views.push(null_tex_man.get(dimension))
    }
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
    views: &[&TextureView],
    dimension: TextureViewDimension,
) -> Arc<BindGroup> {
    Arc::new(device.create_bind_group(&BindGroupDescriptor {
        label: Some(&*format!("{:?} texture bg", dimension)),
        layout,
        entries: &[BindGroupEntry {
            binding: 0,
            resource: BindingResource::TextureViewArray(views),
        }],
    }))
}

struct NullTextureManager {
    null_tex: Texture,
    inner: Vec<TextureView>,
}
impl NullTextureManager {
    pub fn new(device: &Device, dimension: TextureViewDimension) -> Self {
        let null_tex = device.create_texture(&TextureDescriptor {
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
        });

        Self {
            null_tex,
            inner: Vec::new(),
        }
    }

    fn ensure_at_least_one(&mut self, dimension: TextureViewDimension) {
        if self.inner.is_empty() {
            self.inner.push(self.null_tex.create_view(&TextureViewDescriptor {
                dimension: Some(dimension),
                ..TextureViewDescriptor::default()
            }));
        }
    }

    pub fn get(&mut self, dimension: TextureViewDimension) -> TextureView {
        self.ensure_at_least_one(dimension);

        self.inner.pop().unwrap()
    }

    pub fn get_ref(&self) -> &TextureView {
        self.inner.first().unwrap()
    }

    pub fn put(&mut self, view: TextureView) {
        self.inner.push(view);
    }
}
