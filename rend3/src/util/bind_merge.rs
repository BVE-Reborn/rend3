use crate::util::typedefs::SsoString;
use wgpu::{BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Device};

pub struct BindGroupBuilder<'a> {
    label: Option<SsoString>,
    bg_entries: Vec<BindGroupEntry<'a>>,
}
impl<'a> BindGroupBuilder<'a> {
    pub fn new(label: Option<&str>) -> Self {
        Self {
            label: label.map(SsoString::from),
            bg_entries: Vec::with_capacity(16),
        }
    }

    pub fn append(&mut self, resource: BindingResource<'a>) {
        let index = self.bg_entries.len();
        self.bg_entries.push(BindGroupEntry {
            binding: index as u32,
            resource,
        });
    }

    pub fn build(self, device: &Device, bgl: &BindGroupLayout) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: self.label.as_deref(),
            layout: &bgl,
            entries: &self.bg_entries,
        })
    }
}
