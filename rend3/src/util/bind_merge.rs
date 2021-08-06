use crate::util::typedefs::SsoString;
use wgpu::{
    BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindingResource, Buffer, Device, Sampler,
    TextureView,
};

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

    // TODO: remove usages of this in favor of with
    pub fn append(&mut self, resource: BindingResource<'a>) {
        let index = self.bg_entries.len();
        self.bg_entries.push(BindGroupEntry {
            binding: index as u32,
            resource,
        });
    }

    pub fn with(mut self, resource: BindingResource<'a>) -> Self {
        self.append(resource);
        self
    }

    pub fn with_buffer(mut self, buffer: &'a Buffer) -> Self {
        self.append(buffer.as_entire_binding());
        self
    }

    pub fn with_sampler(mut self, sampler: &'a Sampler) -> Self {
        self.append(BindingResource::Sampler(sampler));
        self
    }

    pub fn with_texture_view(mut self, texture_view: &'a TextureView) -> Self {
        self.append(BindingResource::TextureView(texture_view));
        self
    }

    pub fn with_texture_view_array(mut self, texture_view_array: &'a [&'a TextureView]) -> Self {
        self.append(BindingResource::TextureViewArray(texture_view_array));
        self
    }

    pub fn build(self, device: &Device, bgl: &BindGroupLayout) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: self.label.as_deref(),
            layout: &bgl,
            entries: &self.bg_entries,
        })
    }
}
