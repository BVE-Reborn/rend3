use crate::list::{ImageReference, ImageResolution, RenderList, ShaderSource};
use crate::renderer::list::{BufferResource, ImageResource, ShaderResource};
use crate::renderer::shaders::ShaderManager;
use fnv::FnvHashMap;
use futures::stream::FuturesUnordered;
use std::borrow::Cow;
use std::sync::Arc;
use wgpu::{Device, ShaderModuleSource, TextureDescriptor};

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
                if &value.inner.desc == descriptor {
                    continue;
                }
                value.used = true;
            }

            match descriptor {
                ShaderSource::Glsl(source) => {
                    shaders.push(async { (key, shader_manager.compile_shader(source).await) })
                }
                ShaderSource::SpirV(spirv) => {
                    let module = device.create_shader_module(ShaderModuleSource::SpirV(Cow::Owned(spirv)));
                    shaders.push(async { (key, Ok(Arc::new(module))) });
                }
            }
        }

        for (key, descriptor) in list.images {
            if let Some(value) = self.shaders.get_mut(&key) {
                if &value.inner.desc == descriptor {
                    continue;
                }
                value.used = true;
            }

            device.create_texture(&TextureDescriptor {})
        }
    }
}
