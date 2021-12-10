use glam::UVec2;
use rend3::types::SampleCount;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RenderTextureOptions {
    pub resolution: UVec2,
    pub samples: SampleCount,
}
