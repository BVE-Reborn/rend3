//! Rendergraph implementation that rend3 uses for all render work scheduling.
//!
//! Start with [`RenderGraph::new`] and add nodes and then
//! [`RenderGraph::execute`] to run everything.
//!
//! # High Level Overview
//!
//! The design consists of a series of nodes which have inputs and outputs.
//! These inputs can be render targets, shadow targets, or custom user data. The
//! graph is laid out in order using the inputs/outputs then pruned.
//!
//! Each node is a pile of arbitrary code that can use various resources within
//! the renderer to do work.
//!
//! Two submits happen during execute. First, all work that doesn't interact
//! with the surface is submitted, then the surface is acquired, then all the
//! following work is submitted.
//!
//! # Nodes
//!
//! Nodes are made with [`RenderGraphNodeBuilder`]. The builder is used to
//! declare all the dependencies of the node ("outside" the node), then
//! [`RenderGraphNodeBuilder::build`] is called. This takes a callback that
//! contains all the code that will run as part of the node (the "inside").
//!
//! The arguments given to this callback give you all the data you need to do
//! your work, including turning handles-to-dependencies into actual concrete
//! resources. See the documentation for [`RenderGraphNodeBuilder::build`] for a
//! description of the arguments you are provided.
//!
//! # Renderpasses/Encoders
//!
//! The graph will automatically deduplicate renderpasses, such that if there
//! are two nodes in a row that have a compatible renderpass, they will use the
//! same renderpass. An encoder will not be available if a renderpass is in use.
//! This is intentional as there should be as few renderpasses as possible, so
//! you should separate the code that needs a raw encoder from the code that is
//! using a renderpass.
//!
//! Because renderpasses carry with them a lifetime that can cause problems
//! there is a solution for dealing with temporaries: the [`RpassTemporaryPool`].
//! If, inside the node, you need to create a temporary, you can put that temporary on
//! the pool, and it will automatically have lifetime `'rpass`. The temporary is
//! destroyed right after the renderpass is.

use std::ops::Range;

use glam::UVec2;
use rend3_types::{SampleCount, TextureFormat, TextureUsages};
use wgpu::{Color, TextureView};

use crate::util::typedefs::SsoString;

mod data_handle;
mod encpass;
#[allow(clippy::module_inception)] // lmao
mod graph;
mod node;
mod store;
mod temp;
mod texture_store;

pub use data_handle::*;
pub use encpass::*;
pub use graph::*;
pub use node::*;
pub use store::*;
pub use temp::*;
pub(crate) use texture_store::*;

/// Description of a single render target.
#[derive(Debug, Clone)]
pub struct RenderTargetDescriptor {
    pub label: Option<SsoString>,
    pub resolution: UVec2,
    pub depth: u32,
    pub samples: SampleCount,
    pub format: TextureFormat,
    pub usage: TextureUsages,
}
impl RenderTargetDescriptor {
    fn to_core(&self) -> RenderTargetCore {
        RenderTargetCore {
            resolution: self.resolution,
            depth: self.depth,
            samples: self.samples,
            format: self.format,
            usage: self.usage,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RenderTargetCore {
    pub resolution: UVec2,
    pub depth: u32,
    pub samples: SampleCount,
    pub format: TextureFormat,
    pub usage: TextureUsages,
}

/// Requirements to render to a particular shadow map.
///
/// view + size form the start/end of the viewport to render to.
pub struct ShadowTarget<'a> {
    /// View to render to
    pub view: &'a TextureView,
    /// 2D offset in the image.
    pub offset: UVec2,
    /// Size in both dimentions of the viewport
    pub size: u32,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]

enum GraphResource {
    ImportedTexture(usize),
    Texture(usize),
    External,
    Data(usize),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(super) struct TextureRegion {
    idx: usize,
    layer_start: u32,
    layer_end: u32,
    viewport: ViewportRect,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum GraphSubResource {
    ImportedTexture(TextureRegion),
    Texture(TextureRegion),
    External,
    Data(usize),
}

impl GraphSubResource {
    pub(super) fn to_resource(self) -> GraphResource {
        match self {
            GraphSubResource::ImportedTexture(r) => GraphResource::ImportedTexture(r.idx),
            GraphSubResource::Texture(r) => GraphResource::Texture(r.idx),
            GraphSubResource::External => GraphResource::External,
            GraphSubResource::Data(idx) => GraphResource::Data(idx),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ViewportRect {
    pub offset: UVec2,
    pub size: UVec2,
}

impl ViewportRect {
    pub fn new(offset: UVec2, size: UVec2) -> Self {
        Self { offset, size }
    }

    pub fn from_size(size: UVec2) -> Self {
        Self::new(UVec2::ZERO, size)
    }
}

/// Handle to a graph-stored render target.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct RenderTargetHandle {
    // Must only be OutputTexture or Texture
    resource: GraphSubResource,
}

impl RenderTargetHandle {
    pub fn compatible(&self, other: &Self) -> bool {
        let left = self.to_region();
        let right = other.to_region();

        left.idx == right.idx && left.layer_start == right.layer_start && left.layer_end == right.layer_end
    }

    pub(super) fn to_region(self) -> TextureRegion {
        match self.resource {
            GraphSubResource::ImportedTexture(region) | GraphSubResource::Texture(region) => region,
            GraphSubResource::External | GraphSubResource::Data(_) => unreachable!(),
        }
    }

    pub fn restrict(mut self, layers: Range<u32>, viewport: ViewportRect) -> Self {
        match &mut self.resource {
            GraphSubResource::ImportedTexture(region) | GraphSubResource::Texture(region) => {
                region.layer_start = layers.start;
                region.layer_end = layers.end;
                region.viewport = viewport;
            }
            _ => unreachable!(),
        }
        self
    }
}

/// Targets that make up a renderpass.
#[derive(Debug, PartialEq)]
pub struct RenderPassTargets {
    /// Color targets
    pub targets: Vec<RenderPassTarget>,
    /// Depth-stencil target
    pub depth_stencil: Option<RenderPassDepthTarget>,
}

impl RenderPassTargets {
    /// Determines if two renderpasses have compatible targets.
    ///
    /// `this: Some, other: Some` will check the contents  
    /// `this: None, other: None` is always true  
    /// one some and one none is always false.
    pub fn compatible(this: Option<&Self>, other: Option<&Self>) -> bool {
        match (this, other) {
            (Some(this), Some(other)) => {
                let targets_compatible = this.targets.len() == other.targets.len()
                    && this.targets.iter().zip(other.targets.iter()).all(|(me, you)| {
                        let color_compat = me.color.handle.compatible(&you.color.handle);
                        let resolve_compat = match (me.resolve, you.resolve) {
                            (Some(me_dep), Some(you_dep)) => me_dep.handle.compatible(&you_dep.handle),
                            (None, None) => true,
                            _ => false,
                        };
                        color_compat && resolve_compat
                    });

                let depth_compatible = match (&this.depth_stencil, &other.depth_stencil) {
                    (Some(this_depth), Some(other_depth)) => {
                        this_depth.target.handle.compatible(&other_depth.target.handle)
                            && this_depth.depth_clear == other_depth.depth_clear
                            && this_depth.stencil_clear == other_depth.stencil_clear
                    }
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

/// Color target in a renderpass.
#[derive(Debug, PartialEq)]
pub struct RenderPassTarget {
    /// Color attachment. Must be declared as a dependency of the node before it
    /// can be used.
    pub color: DeclaredDependency<RenderTargetHandle>,
    /// Color the attachment will be cleared with if this is the first use.
    pub clear: Color,
    /// Resolve attachment. Can only be present if color attachment has > 1
    /// sample.
    pub resolve: Option<DeclaredDependency<RenderTargetHandle>>,
}

/// Depth target in a renderpass.
#[derive(Debug, PartialEq)]
pub struct RenderPassDepthTarget {
    /// The target to use as depth.
    pub target: DeclaredDependency<RenderTargetHandle>,
    /// Depth value the attachment will be cleared with if this is the first
    /// use.
    pub depth_clear: Option<f32>,
    /// Stencil value the attachment will be cleared with if this is the first
    /// use.
    pub stencil_clear: Option<u32>,
}
