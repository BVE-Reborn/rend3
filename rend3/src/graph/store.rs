use std::{any::Any, cell::RefCell, marker::PhantomData};

use wgpu::TextureView;

use crate::{
    graph::{
        DeclaredDependency, GraphResource, RenderTargetHandle, RpassTemporaryPool, ShadowTarget, ShadowTargetHandle,
    },
    managers::{
        CameraManager, DirectionalLightManager, MaterialManager, MeshManager, ObjectManager, ShadowCoordinates,
        SkeletonManager, TextureManager,
    },
    util::typedefs::FastHashMap,
};

pub struct DataHandle<T> {
    pub(super) resource: GraphResource,
    pub(super) _phantom: PhantomData<T>,
}

impl<T> std::fmt::Debug for DataHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataHandle").field("resource", &self.resource).finish()
    }
}

impl<T> Copy for DataHandle<T> {}

impl<T> Clone for DataHandle<T> {
    fn clone(&self) -> Self {
        Self {
            resource: self.resource,
            _phantom: self._phantom,
        }
    }
}

impl<T> PartialEq for DataHandle<T> {
    fn eq(&self, other: &Self) -> bool {
        self.resource == other.resource && self._phantom == other._phantom
    }
}

pub struct RenderGraphDataStore<'a> {
    pub(super) texture_mapping: &'a FastHashMap<usize, TextureView>,
    pub(super) shadow_coordinates: &'a [ShadowCoordinates],
    pub(super) shadow_views: &'a [TextureView],
    pub(super) data: &'a [Box<dyn Any>], // Any is RefCell<Option<T>> where T is the stored data
    pub(super) output: Option<&'a TextureView>,

    pub camera_manager: &'a CameraManager,
    pub directional_light_manager: &'a DirectionalLightManager,
    pub material_manager: &'a MaterialManager,
    pub mesh_manager: &'a MeshManager,
    pub skeleton_manager: &'a SkeletonManager,
    pub object_manager: &'a ObjectManager,
    pub d2_texture_manager: &'a TextureManager,
    pub d2c_texture_manager: &'a TextureManager,
}

impl<'a> RenderGraphDataStore<'a> {
    pub fn get_render_target(&self, dep: DeclaredDependency<RenderTargetHandle>) -> &'a TextureView {
        match dep.handle.resource {
            GraphResource::Texture(name) => self
                .texture_mapping
                .get(&name)
                .expect("internal rendergraph error: failed to get named texture"),
            GraphResource::OutputTexture => self
                .output
                .expect("internal rendergraph error: tried to get unacquired surface image"),
            r => {
                panic!("internal rendergraph error: tried to get a {:?} as a render target", r)
            }
        }
    }

    pub fn get_shadow(&self, handle: ShadowTargetHandle) -> ShadowTarget<'_> {
        let coords = self
            .shadow_coordinates
            .get(handle.idx)
            .expect("internal rendergraph error: failed to get shadow mapping");
        ShadowTarget {
            view: self
                .shadow_views
                .get(coords.layer)
                .expect("internal rendergraph error: failed to get shadow layer"),
            offset: coords.offset,
            size: coords.size,
        }
    }

    pub fn set_data<T: 'static>(&self, dep: DeclaredDependency<DataHandle<T>>, data: Option<T>) {
        match dep.handle.resource {
            GraphResource::Data(idx) => {
                *self
                    .data
                    .get(idx)
                    .expect("internal rendergraph error: failed to get buffer")
                    .downcast_ref::<RefCell<Option<T>>>()
                    .expect("internal rendergraph error: downcasting failed")
                    .try_borrow_mut()
                    .expect("tried to call set_data on a handle that has an outstanding borrow through get_data") = data
            }
            r => {
                panic!("internal rendergraph error: tried to get a {:?} as a render target", r)
            }
        }
    }

    pub fn get_data<T: 'static>(
        &self,
        temps: &'a RpassTemporaryPool<'a>,
        dep: DeclaredDependency<DataHandle<T>>,
    ) -> Option<&'a T> {
        match dep.handle.resource {
            GraphResource::Data(idx) => temps
                .add(
                    self.data
                        .get(idx)
                        .expect("internal rendergraph error: failed to get buffer")
                        .downcast_ref::<RefCell<Option<T>>>()
                        .expect("internal rendergraph error: downcasting failed")
                        .try_borrow()
                        .expect("internal rendergraph error: read-only borrow failed"),
                )
                .as_ref(),
            r => {
                panic!("internal rendergraph error: tried to get a {:?} as a render target", r)
            }
        }
    }
}
