use crate::{
    list::{RenderList, ShaderSource},
    renderer::{
        list::{BufferResource, ImageResource, ShaderResource},
        shaders::{ShaderCompileResult, ShaderManager},
    },
};
use fnv::FnvHashMap;
use futures::{future::Either, stream::FuturesUnordered};
use std::{borrow::Cow, sync::Arc};
use wgpu::{Device, Extent3d, ShaderModuleSource, TextureDescriptor, TextureDimension, TextureViewDescriptor};

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

    pub async fn add_render_list(&mut self, device: &Device, shader_manager: &ShaderManager, list: RenderList) {
        self.mark_all_unused();

        let shaders = FuturesUnordered::new();
        for (key, descriptor) in list.shaders {
            if let Some(value) = self.shaders.get_mut(&key) {
                if value.inner.desc == descriptor {
                    continue;
                }
                value.used = true;
            }

            match descriptor {
                ShaderSource::Glsl(source) => shaders.push(Either::Left(async {
                    (key, shader_manager.compile_shader(source).await)
                })),
                ShaderSource::SpirV(spirv) => {
                    let module = device.create_shader_module(ShaderModuleSource::SpirV(Cow::Owned(spirv)));
                    shaders.push(Either::Right(async {
                        (key, ShaderCompileResult::Ok(Arc::new(module)))
                    }));
                }
            }
        }

        for (key, descriptor) in list.images {
            if let Some(value) = self.images.get_mut(&key) {
                if value.inner.desc == descriptor {
                    continue;
                }
                value.used = true;
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
    }
}
