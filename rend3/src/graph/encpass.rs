use std::mem;

use wgpu::{CommandEncoder, RenderPass};
use wgpu_profiler::ProfilerCommandRecorder;

use crate::graph::DeclaredDependency;

/// Handle to a declared renderpass output.
pub struct RenderPassHandle;

#[derive(Default)]
pub(super) enum RenderGraphEncoderOrPassInner<'a, 'pass> {
    Encoder(&'a mut CommandEncoder),
    RenderPass(&'a mut RenderPass<'pass>),
    #[default]
    None,
}

impl<'a, 'pass> ProfilerCommandRecorder for RenderGraphEncoderOrPassInner<'a, 'pass> {
    fn is_pass(&self) -> bool {
        matches!(self, RenderGraphEncoderOrPassInner::RenderPass(_))
    }

    fn write_timestamp(&mut self, query_set: &wgpu::QuerySet, query_index: u32) {
        match self {
            RenderGraphEncoderOrPassInner::Encoder(e) => e.write_timestamp(query_set, query_index),
            RenderGraphEncoderOrPassInner::RenderPass(rp) => rp.write_timestamp(query_set, query_index),
            RenderGraphEncoderOrPassInner::None => panic!("Already removed the contents of RenderGraphEncoderOrPass"),
        }
    }

    fn push_debug_group(&mut self, label: &str) {
        match self {
            RenderGraphEncoderOrPassInner::Encoder(e) => e.push_debug_group(label),
            RenderGraphEncoderOrPassInner::RenderPass(rp) => rp.push_debug_group(label),
            RenderGraphEncoderOrPassInner::None => panic!("Already removed the contents of RenderGraphEncoderOrPass"),
        }
    }

    fn pop_debug_group(&mut self) {
        match self {
            RenderGraphEncoderOrPassInner::Encoder(e) => e.pop_debug_group(),
            RenderGraphEncoderOrPassInner::RenderPass(rp) => rp.pop_debug_group(),
            RenderGraphEncoderOrPassInner::None => panic!("Already removed the contents of RenderGraphEncoderOrPass"),
        }
    }
}

/// Holds either a renderpass or an encoder.
pub struct RenderGraphEncoderOrPass<'a, 'pass>(pub(super) RenderGraphEncoderOrPassInner<'a, 'pass>);

impl<'a, 'pass> RenderGraphEncoderOrPass<'a, 'pass> {
    /// Takes the encoder out of this struct.
    ///
    /// # Panics
    ///
    /// - If this node requested a renderpass.
    /// - If a take_* function is called twice.
    pub fn take_encoder(&mut self) -> &'a mut CommandEncoder {
        match mem::take(&mut self.0) {
            RenderGraphEncoderOrPassInner::Encoder(e) => e,
            RenderGraphEncoderOrPassInner::RenderPass(_) => {
                panic!("called get_encoder when the rendergraph node asked for a renderpass");
            }
            RenderGraphEncoderOrPassInner::None => panic!("Already removed the contents of RenderGraphEncoderOrPass"),
        }
    }

    /// Takes the renderpass out of this struct using the given handle.
    ///
    /// # Panics
    ///
    /// - If a take_* function is called twice.
    pub fn take_rpass(&mut self, _handle: DeclaredDependency<RenderPassHandle>) -> &'a mut RenderPass<'pass> {
        match mem::take(&mut self.0) {
            RenderGraphEncoderOrPassInner::Encoder(_) => {
                panic!("Internal rendergraph error: trying to get renderpass when one was not asked for")
            }
            RenderGraphEncoderOrPassInner::RenderPass(rpass) => rpass,
            RenderGraphEncoderOrPassInner::None => panic!("Already removed the contents of RenderGraphEncoderOrPass"),
        }
    }
}
