use wgpu::{CommandEncoder, Device};

use crate::{RendererMode, resources::{InternalObject, MaterialManager}};

use super::{culling::CullingOutput, CacheContext};

pub fn shadow_pass_culling(
    mode: RendererMode,
    device: &Device,
    ctx: &CacheContext,
    encoder: &mut CommandEncoder,
    material: &MaterialManager,
    objects: &[InternalObject],
) -> Vec<CullingOutput> {
    todo!()
}
