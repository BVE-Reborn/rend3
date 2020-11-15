use crate::{
    list::{RenderList, ShaderSource},
    renderer::{
        list::{BufferResource, ImageResource, ShaderResource},
        shaders::{ShaderCompileResult, ShaderManager},
    },
};
use fnv::FnvHashMap;
use futures::{future::Either, stream::FuturesUnordered, StreamExt};
use std::{borrow::Cow, sync::Arc};
use wgpu::{Device, Extent3d, ShaderModuleSource, TextureDescriptor, TextureDimension, TextureViewDescriptor, BufferDescriptor};

pub struct RenderListCacheResource<T> {
    pub inner: T,
    pub used: bool,
}

pub struct RenderListCache {
    shaders: FnvHashMap<String, RenderListCacheResource<ShaderResource>>,
    images: FnvHashMap<String, RenderListCacheResource<ImageResource>>,
    buffers: FnvHashMap<String, RenderListCacheResource<BufferResource>>,
}

impl RenderListCache {
    pub fn new() -> Self {
        Self {
            shaders: FnvHashMap::default(),
            images: FnvHashMap::default(),
            buffers: FnvHashMap::default(),
        }
    }

    fn mark_all_unused(&mut self) {
        for shader in self.shaders.values_mut() {
            shader.used = false;
        }
        for image in self.images.values_mut() {
            image.used = false;
        }
        for buffers in self.buffers.values_mut() {
            buffers.used = false;
        }
    }

    fn purge_unused_resources(&mut self) {
        self.shaders.retain(|_, s| s.used);
        self.images.retain(|_, i| i.used);
        self.buffers.retain(|_, b| b.used);
    }

    pub async fn add_render_list(&mut self, device: &Device, shader_manager: &ShaderManager, list: RenderList) {
        self.mark_all_unused();

        let mut shaders = FuturesUnordered::new();
        for (key, descriptor) in list.shaders {
            if let Some(value) = self.shaders.get_mut(&key) {
                if value.inner.desc == descriptor {
                    value.used = true;
                    continue;
                }
            }

            match descriptor {
                ShaderSource::Glsl(ref source) => {
                    let shader_future = shader_manager.compile_shader(source.clone());
                    shaders.push(Either::Left(async {
                        (key, descriptor,  shader_future.await)
                    }))
                },
                ShaderSource::SpirV(ref spirv) => {
                    let module = device.create_shader_module(ShaderModuleSource::SpirV(Cow::Borrowed(spirv)));
                    shaders.push(Either::Right(async {
                        (key, descriptor, ShaderCompileResult::Ok(Arc::new(module)))
                    }));
                }
            }
        }

        for (key, descriptor) in list.images {
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

        for (key, descriptor) in list.buffers {
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
                mapped_at_creation: false
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

        while let Some((key, desc, result)) = shaders.next().await {
            self.shaders.insert(key, RenderListCacheResource {
                inner: ShaderResource {
                    desc,
                    shader: result.unwrap()
                },
                used: true,
            });
        }

        self.purge_unused_resources();
    }
}
