use crate::{
    datatypes::{RendererTextureFormat, TextureHandle},
    registry::ResourceRegistry,
    renderer::{ModeData, RendererMode},
};
use std::{mem, num::NonZeroU32, sync::Arc};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, Device, Extent3d, ShaderStage, Texture, TextureComponentType, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsage, TextureView, TextureViewDescriptor, TextureViewDimension,
};

pub const STARTING_2D_TEXTURES: usize = 1 << 6;
pub const STARTING_CUBE_TEXTURES: usize = 1 << 3;

pub struct InternalTexture {
    pub format: Option<RendererTextureFormat>,
}

pub struct TextureManager {
    layout: ModeData<(), Arc<BindGroupLayout>>,
    layout_dirty: ModeData<(), bool>,

    group: ModeData<(), Arc<BindGroup>>,
    group_dirty: ModeData<(), bool>,

    null_tex_man: NullTextureManager,

    views: Vec<TextureView>,
    registry: ResourceRegistry<InternalTexture>,

    dimension: TextureViewDimension,
}
impl TextureManager {
    pub fn new(device: &Device, mode: RendererMode, starting_textures: usize, dimension: TextureViewDimension) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Texture Manager");

        let mut null_tex_man = NullTextureManager::new(device, dimension);

        let view_count = starting_textures;

        let mut views = Vec::with_capacity(view_count);
        fill_to_size(&mut null_tex_man, &mut views, dimension, view_count);

        let layout = mode.into_data(|| (), || create_bind_group_layout(device, view_count as u32, dimension));
        let group = mode.into_data(|| (), || create_bind_group(device, layout.as_gpu(), &views, dimension));

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
        TextureHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: TextureHandle, texture: TextureView, format: Option<RendererTextureFormat>) {
        span_transfer!(_ -> fill_span, INFO, "Texture Manager Fill");

        self.group_dirty = self.group_dirty.map_gpu(|_| true);

        let index = self.registry.insert(handle.0, InternalTexture { format });

        if index > self.views.len() {
            self.layout_dirty = self.layout_dirty.map_gpu(|_| true);

            let new_size = self.views.len() * 2;
            fill_to_size(&mut self.null_tex_man, &mut self.views, self.dimension, new_size);
        }

        let old_null = mem::replace(&mut self.views[index], texture);

        self.null_tex_man.put(old_null);
    }

    pub fn remove(&mut self, handle: TextureHandle) {
        span_transfer!(_ -> remove_span, INFO, "Material Manager Remove");

        let (index, _) = self.registry.remove(handle.0);

        let active_count = self.registry.count();

        // Do the same swap remove move as the registry did
        if active_count > 1 {
            self.views.swap(index, active_count);
        }
        // Overwrite the last item with the null tex
        self.views[active_count] = self.null_tex_man.get(self.dimension);
    }

    pub fn internal_index(&self, handle: TextureHandle) -> usize {
        self.registry.get_index_of(handle.0)
    }

    pub fn ready(
        &mut self,
        device: &Device,
    ) -> (
        ModeData<(), Arc<BindGroupLayout>>,
        ModeData<(), Arc<BindGroup>>,
        ModeData<(), bool>,
    ) {
        span_transfer!(_ -> ready_span, INFO, "Material Manager Ready");

        if let ModeData::GPU(_) = self.layout_dirty {
            let layout_dirty = self.layout_dirty;

            if self.layout_dirty.into_gpu() {
                *self.layout.as_gpu_mut() = create_bind_group_layout(device, self.views.len() as u32, self.dimension);
                *self.layout_dirty.as_gpu_mut() = false;
            }

            if self.group_dirty.into_gpu() {
                *self.group.as_gpu_mut() = create_bind_group(device, self.layout.as_gpu(), &self.views, self.dimension);
                *self.group_dirty.as_gpu_mut() = false;
            }

            (
                self.layout.map_gpu(Arc::clone),
                self.group.map_gpu(Arc::clone),
                layout_dirty,
            )
        } else {
            (ModeData::CPU(()), ModeData::CPU(()), ModeData::CPU(()))
        }
    }

    pub fn get(&self, handle: TextureHandle) -> &InternalTexture {
        &self.registry.get(handle.0)
    }

    pub fn get_view(&self, handle: TextureHandle) -> &TextureView {
        &self.views[self.registry.get_index_of(handle.0)]
    }

    pub fn ensure_null_view(&mut self) {
        self.null_tex_man.ensure_at_least_one(self.dimension)
    }

    pub fn get_null_view(&self) -> &TextureView {
        self.null_tex_man.get_ref()
    }

    pub fn gpu_bind_group_layout(&self) -> &BindGroupLayout {
        self.layout.as_gpu()
    }

    pub fn translation_fn(&self) -> impl Fn(TextureHandle) -> NonZeroU32 + Copy + '_ {
        move |v: TextureHandle| unsafe {
            // SAFETY: overflowing this number will panic
            NonZeroU32::new_unchecked(self.internal_index(v) as u32 + 1)
        }
    }
}

fn fill_to_size(
    null_tex_man: &mut NullTextureManager,
    views: &mut Vec<TextureView>,
    dimension: TextureViewDimension,
    size: usize,
) {
    span_transfer!(_ -> fill_span, INFO, "fill to size");

    let to_add = size.saturating_sub(views.len());

    for _ in 0..to_add {
        views.push(null_tex_man.get(dimension))
    }
}

fn create_bind_group_layout(device: &Device, count: u32, dimension: TextureViewDimension) -> Arc<BindGroupLayout> {
    Arc::new(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some(&*format!("{:?} texture bgl", dimension)),
        entries: &[BindGroupLayoutEntry {
            binding: 0,
            visibility: ShaderStage::FRAGMENT,
            ty: BindingType::SampledTexture {
                dimension,
                component_type: TextureComponentType::Float,
                multisampled: false,
            },
            count: NonZeroU32::new(count),
        }],
    }))
}

fn create_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    views: &[TextureView],
    dimension: TextureViewDimension,
) -> Arc<BindGroup> {
    Arc::new(device.create_bind_group(&BindGroupDescriptor {
        label: Some(&*format!("{:?} texture bg", dimension)),
        layout: &layout,
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
        span_transfer!(_ -> new_span, INFO, "Material Manager Ready");

        let null_tex = device.create_texture(&TextureDescriptor {
            label: Some(&*format!("null {:?} texture", dimension)),
            size: Extent3d {
                width: 1,
                height: 1,
                depth: match dimension {
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
            usage: TextureUsage::SAMPLED,
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
        span_transfer!(_ -> get_span, INFO, "Null Texture Manager Get");

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
