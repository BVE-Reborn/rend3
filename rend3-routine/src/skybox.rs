use rend3::types::TextureHandle;
use wgpu::{BindGroup, RenderPipeline};

pub struct StoredSkybox {
    pub bg: BindGroup,
    pub handle: TextureHandle,
}

pub struct SkyboxPass {
    pub skybox_pipeline: RenderPipeline,
    pub current_skybox: Option<StoredSkybox>,
}

impl SkyboxPass {
    pub fn new(skybox_pipeline: RenderPipeline) -> Self {
        Self {
            skybox_pipeline,
            current_skybox: None,
        }
    }
}
