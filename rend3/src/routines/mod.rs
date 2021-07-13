use crate::cache::{BindGroupCache, PipelineCache, ShaderModuleCache};

pub struct CacheContext<'a> {
    pub sm_cache: &'a mut ShaderModuleCache,
    pub pipeline_cache: &'a mut PipelineCache,
    pub bind_group_cache: &'a mut BindGroupCache,
}

mod culling;
mod shadow;