use crate::{
    modules::{MeshManager, TextureManager},
    renderer::info::ExtendedAdapterInfo,
};
use crate::{Renderer, RendererMode};
use parking_lot::RwLock;
use std::sync::Arc;
use wgpu::{Device, Queue};

use super::resources::RendererGlobalResources;

#[derive(Clone)]
pub struct RenderContext<TLD>
where
    TLD: 'static,
{
    renderer: Arc<Renderer<TLD>>,
}
impl<TLD> RenderContext<TLD>
where
    TLD: 'static,
{
    pub fn mode(&self) -> RendererMode {
        self.renderer.mode
    }

    pub fn device(&self) -> &Arc<Device> {
        &self.renderer.device
    }

    pub fn queue(&self) -> &Arc<Queue> {
        &self.renderer.queue
    }

    pub fn adapter_info(&self) -> ExtendedAdapterInfo {
        self.renderer.adapter_info
    }

    pub fn global_resources(&self) -> &RwLock<RendererGlobalResources> {
        &self.renderer.global_resources
    }

    pub fn mesh_manager(&self) -> &RwLock<MeshManager> {
        &self.renderer.mesh_manager
    }

    pub fn texture_manager_2d(&self) -> &RwLock<TextureManager> {
        &self.renderer.texture_manager_2d
    }
}
