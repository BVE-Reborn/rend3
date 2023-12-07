use std::{cell::RefCell, marker::PhantomData, sync::Arc};

use wgpu::{Texture, TextureView};

use crate::{
    graph::{
        DataContents, DeclaredDependency, GraphSubResource, RenderTargetHandle, RpassTemporaryPool, TextureRegion,
    },
    util::typedefs::FastHashMap,
};

/// Handle to arbitrary graph-stored data.
pub struct DataHandle<T> {
    pub(super) idx: usize,
    pub(super) _phantom: PhantomData<T>,
}

impl<T> std::fmt::Debug for DataHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataHandle").field("idx", &self.idx).finish()
    }
}

impl<T> Copy for DataHandle<T> {}

impl<T> Clone for DataHandle<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for DataHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx && self._phantom == other._phantom
    }
}

/// Provides read-only access to the renderer and access to graph resources.
///
/// This is how you turn [DeclaredDependency] into actual wgpu resources.
pub struct RenderGraphDataStore<'a> {
    pub(super) texture_mapping: &'a FastHashMap<TextureRegion, (TextureView, Arc<Texture>)>,
    pub(super) external_texture_mapping: &'a FastHashMap<TextureRegion, TextureView>,
    pub(super) data: &'a [DataContents], // Any is RefCell<Option<T>> where T is the stored data
}

impl<'a> RenderGraphDataStore<'a> {
    /// Get a rendertarget as a TextureView from the handle to one.
    pub fn get_render_target(&self, dep: DeclaredDependency<RenderTargetHandle>) -> &'a TextureView {
        match dep.handle.resource {
            GraphSubResource::Texture(name) => {
                &self
                    .texture_mapping
                    .get(&name)
                    .expect("internal rendergraph error: failed to get named texture")
                    .0
            }
            GraphSubResource::ImportedTexture(name) => self
                .external_texture_mapping
                .get(&name)
                .expect("internal rendergraph error: failed to get named texture"),
            r => {
                panic!("internal rendergraph error: tried to get a {r:?} as a render target")
            }
        }
    }

    /// Get a rendertarget as a Texture from the handle to one
    pub fn get_render_target_texture(&self, dep: DeclaredDependency<RenderTargetHandle>) -> &'a Texture {
        match dep.handle.resource {
            GraphSubResource::Texture(name) => {
                &self
                    .texture_mapping
                    .get(&name)
                    .expect("internal rendergraph error: failed to get named texture")
                    .1
            }
            GraphSubResource::ImportedTexture(_) => {
                panic!("Getting render target as a texture not supported for imported textures");
            }
            r => {
                panic!("internal rendergraph error: tried to get a {r:?} as a render target")
            }
        }
    }

    /// Set the custom data behind a data handle.
    ///
    /// # Panics
    ///
    /// If get_data was called in the same renderpass, calling this will panic.
    pub fn set_data<T: 'static>(&self, dep: DeclaredDependency<DataHandle<T>>, data: Option<T>) {
        *self
            .data
            .get(dep.handle.idx)
            .expect("internal rendergraph error: failed to get buffer")
            .inner
            .downcast_ref::<RefCell<Option<T>>>()
            .expect("internal rendergraph error: downcasting failed")
            .try_borrow_mut()
            .expect("tried to call set_data on a handle that has an outstanding borrow through get_data") = data
    }

    /// Gets the custom data behind a data handle. If it has not been set, or
    /// set to None, this will return None.
    pub fn get_data<T: 'static>(
        &self,
        temps: &'a RpassTemporaryPool<'a>,
        dep: DeclaredDependency<DataHandle<T>>,
    ) -> Option<&'a T> {
        let borrow = self
            .data
            .get(dep.handle.idx)
            .expect("internal rendergraph error: failed to get buffer")
            .inner
            .downcast_ref::<RefCell<Option<T>>>()
            .expect("internal rendergraph error: downcasting failed")
            .try_borrow()
            .expect("internal rendergraph error: read-only borrow failed");
        match *borrow {
            Some(_) => {
                let r = temps.add(borrow);
                Some(r.as_ref().unwrap())
            }
            None => None,
        }
    }
}
