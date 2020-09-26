use crate::{datatypes::TextureHandle, registry::ResourceRegistry};
use std::{mem, num::NonZeroU32, sync::Arc};
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry,
    BindingResource, BindingType, Device, Extent3d, Sampler, ShaderStage, Texture, TextureComponentType,
    TextureDescriptor, TextureDimension, TextureFormat, TextureUsage, TextureView, TextureViewDescriptor,
    TextureViewDimension,
};

const STARTING_TEXTURES: usize = 1 << 8;

pub struct TextureManager {
    layout: Arc<BindGroupLayout>,
    layout_dirty: bool,

    group: Arc<BindGroup>,
    group_dirty: bool,

    null_tex_man: NullTextureManager,

    views: Vec<TextureView>,
    registry: ResourceRegistry<()>,
}
impl TextureManager {
    pub fn new(device: &Device, sampler: &Sampler) -> Self {
        span_transfer!(_ -> new_span, INFO, "Creating Texture Manager");

        let mut null_tex_man = NullTextureManager::new(device);

        let view_count = STARTING_TEXTURES;

        let mut views = Vec::with_capacity(view_count);
        fill_to_size(&mut null_tex_man, &mut views, view_count);

        let layout = create_bind_group_layout(device, view_count as u32);
        let group = create_bind_group(device, &layout, &views, sampler);

        let registry = ResourceRegistry::new();

        Self {
            layout,
            layout_dirty: false,
            group,
            group_dirty: false,
            null_tex_man,
            views,
            registry,
        }
    }

    pub fn allocate(&self) -> TextureHandle {
        TextureHandle(self.registry.allocate())
    }

    pub fn fill(&mut self, handle: TextureHandle, texture: TextureView) {
        span_transfer!(_ -> fill_span, INFO, "Texture Manager Fill");

        self.group_dirty = true;

        let index = self.registry.insert(handle.0, ());

        if index > self.views.len() {
            self.layout_dirty = true;

            let new_size = self.views.len() * 2;
            fill_to_size(&mut self.null_tex_man, &mut self.views, new_size);
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
        self.views[active_count] = self.null_tex_man.get();
    }

    pub fn internal_index(&self, handle: TextureHandle) -> usize {
        self.registry.get_index_of(handle.0)
    }

    pub fn ready(&mut self, device: &Device, sampler: &Sampler) -> (Option<Arc<BindGroupLayout>>, Arc<BindGroup>) {
        span_transfer!(_ -> ready_span, INFO, "Material Manager Ready");

        let layout = if self.layout_dirty {
            self.layout = create_bind_group_layout(device, self.views.len() as u32);
            self.layout_dirty = false;
            Some(Arc::clone(&self.layout))
        } else {
            None
        };

        if self.group_dirty {
            self.group = create_bind_group(device, &self.layout, &self.views, sampler);
            self.group_dirty = false;
        }

        (layout, Arc::clone(&self.group))
    }

    pub fn bind_group_layout(&self) -> &BindGroupLayout {
        &self.layout
    }
}

fn fill_to_size(null_tex_man: &mut NullTextureManager, views: &mut Vec<TextureView>, size: usize) {
    span_transfer!(_ -> fill_span, INFO, "fill to size");

    let to_add = size.saturating_sub(views.len());

    for _ in 0..to_add {
        views.push(null_tex_man.get())
    }
}

fn create_bind_group_layout(device: &Device, count: u32) -> Arc<BindGroupLayout> {
    Arc::new(device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("texture bindings layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::SampledTexture {
                    dimension: TextureViewDimension::D2,
                    component_type: TextureComponentType::Float,
                    multisampled: false,
                },
                count: NonZeroU32::new(count),
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStage::FRAGMENT,
                ty: BindingType::Sampler { comparison: false },
                count: None,
            },
        ],
    }))
}

fn create_bind_group(
    device: &Device,
    layout: &BindGroupLayout,
    views: &[TextureView],
    sampler: &Sampler,
) -> Arc<BindGroup> {
    Arc::new(device.create_bind_group(&BindGroupDescriptor {
        label: Some("texture binding"),
        layout: &layout,
        entries: &[
            BindGroupEntry {
                binding: 0,
                resource: BindingResource::TextureViewArray(views),
            },
            BindGroupEntry {
                binding: 1,
                resource: BindingResource::Sampler(sampler),
            },
        ],
    }))
}

struct NullTextureManager {
    null_tex: Texture,
    inner: Vec<TextureView>,
}
impl NullTextureManager {
    pub fn new(device: &Device) -> Self {
        span_transfer!(_ -> new_span, INFO, "Material Manager Ready");

        let null_tex = device.create_texture(&TextureDescriptor {
            label: Some("null texture"),
            size: Extent3d::default(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsage::SAMPLED,
        });

        Self {
            null_tex,
            inner: Vec::new(),
        }
    }

    pub fn get(&mut self) -> TextureView {
        span_transfer!(_ -> get_span, INFO, "Null Texture Manager Get");

        self.inner
            .pop()
            .unwrap_or_else(|| self.null_tex.create_view(&TextureViewDescriptor::default()))
    }

    pub fn put(&mut self, view: TextureView) {
        self.inner.push(view);
    }
}
