use crate::{
    list::RenderListResources,
    renderer::list::{BufferResource, ImageResource},
};
use fnv::FnvHashMap;
use std::sync::Arc;
use wgpu::{
    Buffer, BufferDescriptor, Device, Extent3d, TextureDescriptor, TextureDimension, TextureView, TextureViewDescriptor,
};

pub(crate) struct RenderListCacheResource<T> {
    pub inner: T,
    pub used: bool,
}

pub(crate) struct RenderListCache {
    images: FnvHashMap<String, RenderListCacheResource<ImageResource>>,
    buffers: FnvHashMap<String, RenderListCacheResource<BufferResource>>,
}

impl RenderListCache {
    pub fn new() -> Self {
        Self {
            images: FnvHashMap::default(),
            buffers: FnvHashMap::default(),
        }
    }

    fn mark_all_unused(&mut self) {
        for image in self.images.values_mut() {
            image.used = false;
        }
        for buffers in self.buffers.values_mut() {
            buffers.used = false;
        }
    }

    fn purge_unused_resources(&mut self) {
        self.images.retain(|_, i| i.used);
        self.buffers.retain(|_, b| b.used);
    }

    pub fn add_render_list(&mut self, device: &Device, resources: RenderListResources) {
        self.mark_all_unused();

        for (key, descriptor) in resources.images {
            if let Some(value) = self.images.get_mut(&key) {
                if value.inner.desc == descriptor {
                    value.used = true;
                    continue;
                }
            }

            let image = device.create_texture(&TextureDescriptor {
                label: Some(&*key),
                size: Extent3d {
                    width: descriptor.resolution[0],
                    height: descriptor.resolution[1],
                    depth: 1,
                },
                // TODO: mips
                mip_level_count: 1,
                sample_count: descriptor.samples,
                dimension: TextureDimension::D2,
                format: descriptor.format,
                usage: descriptor.usage,
            });

            let image_view = image.create_view(&TextureViewDescriptor::default());

            self.images.insert(
                key,
                RenderListCacheResource {
                    inner: ImageResource {
                        desc: descriptor,
                        image: Arc::new(image),
                        image_view: Arc::new(image_view),
                    },
                    used: true,
                },
            );
        }

        for (key, descriptor) in resources.buffers {
            if let Some(value) = self.buffers.get_mut(&key) {
                if value.inner.desc == descriptor {
                    value.used = true;
                    continue;
                }
            }

            let buffer = device.create_buffer(&BufferDescriptor {
                label: Some(&*key),
                size: descriptor.size as u64,
                usage: descriptor.usage,
                mapped_at_creation: false,
            });

            self.buffers.insert(
                key,
                RenderListCacheResource {
                    inner: BufferResource {
                        desc: descriptor,
                        buffer: Arc::new(buffer),
                    },
                    used: true,
                },
            );
        }

        self.purge_unused_resources();
    }

    pub fn get_buffer(&self, name: &str) -> &Buffer {
        &*self.buffers.get(name).unwrap().inner.buffer
    }

    pub fn get_image(&self, name: &str) -> &TextureView {
        &*self.images.get(name).unwrap().inner.image_view
    }
}
