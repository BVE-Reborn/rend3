use wgpu::{RenderPipeline, Device};
use crate::list::{ResourceBinding, RenderOpInputType};

pub struct PipelineArguments {
    name: String,
    bindings: Vec<ResourceBinding>,
    input: RenderOpInputType,
    vertex: String,
    frag: Option<String>,

}

impl PipelineArguments {
    pub fn create_pipeline_from(&self, device: &Device) -> RenderPipeline {
        unimplemented!()
    }
}