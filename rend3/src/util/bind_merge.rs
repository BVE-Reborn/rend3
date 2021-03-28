use crate::{cache::BindGroupCache, util::typedefs::SsoString};
use std::{num::NonZeroU32, sync::Arc};
use wgpu::{BindGroup, BindGroupEntry, BindGroupLayoutEntry, BindingResource, BindingType, Device, ShaderStage};

pub struct BindGroupBuilder<'a> {
    label: Option<SsoString>,
    bgl_entries: Vec<BindGroupLayoutEntry>,
    bg_entries: Vec<BindGroupEntry<'a>>,
}
impl<'a> BindGroupBuilder<'a> {
    fn new_inner<L>(label: Option<L>) -> Self
    where
        SsoString: From<L>,
    {
        Self {
            label: label.map(SsoString::from),
            bgl_entries: Vec::with_capacity(16),
            bg_entries: Vec::with_capacity(16),
        }
    }

    pub fn new<L>(label: L) -> Self
    where
        SsoString: From<L>,
    {
        Self::new_inner(Some(label))
    }

    pub fn new_no_label() -> Self
    {
        Self::new_inner::<&str>(None)
    }

    pub fn append(
        &mut self,
        visibility: ShaderStage,
        ty: BindingType,
        count: Option<NonZeroU32>,
        resource: BindingResource<'a>,
    ) {
        let index = self.bgl_entries.len();
        self.bgl_entries.push(BindGroupLayoutEntry {
            binding: index as u32,
            visibility,
            ty,
            count,
        });
        self.bg_entries.push(BindGroupEntry {
            binding: index as u32,
            resource,
        });
    }

    pub fn build(self, device: &Device, cache: &mut BindGroupCache) -> Arc<BindGroup> {
        cache.bind_group(device, self.label.as_deref(), &self.bgl_entries, &self.bg_entries)
    }
}
