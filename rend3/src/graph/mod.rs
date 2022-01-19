//! Rendergraph implementation that rend3 uses for all render work scheduling.

use glam::UVec2;
use rend3_types::{BufferUsages, SampleCount, TextureFormat, TextureUsages};
use wgpu::{Color, TextureView};

use crate::util::typedefs::SsoString;

mod encpass;
mod graph;
mod node;
mod passthrough;
mod store;
mod temp;

pub use encpass::*;
pub use graph::*;
pub use node::*;
pub use passthrough::*;
pub use store::*;
pub use temp::*;

#[derive(Debug, Clone)]
pub struct RenderTargetDescriptor {
    pub label: Option<SsoString>,
    pub resolution: UVec2,
    pub samples: SampleCount,
    pub format: TextureFormat,
    pub usage: TextureUsages,
}
impl RenderTargetDescriptor {
    fn to_core(&self) -> RenderTargetCore {
        RenderTargetCore {
            resolution: self.resolution,
            samples: self.samples,
            format: self.format,
            usage: self.usage,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RenderTargetCore {
    pub resolution: UVec2,
    pub samples: SampleCount,
    pub format: TextureFormat,
    pub usage: TextureUsages,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BufferTargetDescriptor {
    pub label: Option<SsoString>,
    pub length: u64,
    pub usage: BufferUsages,
    pub mapped: bool,
}

pub struct ShadowTarget<'a> {
    pub view: &'a TextureView,
    pub offset: UVec2,
    pub size: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum GraphResource {
    OutputTexture,
    External,
    Texture(usize),
    Shadow(usize),
    Data(usize),
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct RenderTargetHandle {
    resource: GraphResource,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ShadowTargetHandle {
    idx: usize,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ShadowArrayHandle;

#[derive(Debug, PartialEq)]
pub struct RenderPassTargets {
    pub targets: Vec<RenderPassTarget>,
    pub depth_stencil: Option<RenderPassDepthTarget>,
}

impl RenderPassTargets {
    pub fn compatible(this: Option<&Self>, other: Option<&Self>) -> bool {
        match (this, other) {
            (Some(this), Some(other)) => {
                let targets_compatible = this.targets.len() == other.targets.len()
                    && this
                        .targets
                        .iter()
                        .zip(other.targets.iter())
                        .all(|(me, you)| me.color == you.color && me.resolve == you.resolve);

                let depth_compatible = match (&this.depth_stencil, &other.depth_stencil) {
                    (Some(this_depth), Some(other_depth)) => this_depth == other_depth,
                    (None, None) => true,
                    _ => false,
                };

                targets_compatible && depth_compatible
            }
            (None, None) => true,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct RenderPassTarget {
    pub color: DeclaredDependency<RenderTargetHandle>,
    pub clear: Color,
    pub resolve: Option<DeclaredDependency<RenderTargetHandle>>,
}

#[derive(Debug, PartialEq)]
pub struct RenderPassDepthTarget {
    pub target: DepthHandle,
    pub depth_clear: Option<f32>,
    pub stencil_clear: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub enum DepthHandle {
    RenderTarget(DeclaredDependency<RenderTargetHandle>),
    Shadow(ShadowTargetHandle),
}
