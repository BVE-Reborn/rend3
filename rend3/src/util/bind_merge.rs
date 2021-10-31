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

    pub fn append(&mut self, resource: BindingResource<'a>) -> &mut Self {
        let index = self.bg_entries.len();
        self.bg_entries.push(BindGroupEntry {
            binding: index as u32,
            resource,
        });
        self
    }

    pub fn append_buffer(&mut self, buffer: &'a Buffer) -> &mut Self {
        self.append(buffer.as_entire_binding());
        self
    }

    pub fn append_sampler(&mut self, sampler: &'a Sampler) -> &mut Self {
        self.append(BindingResource::Sampler(sampler));
        self
    }

    pub fn append_texture_view(&mut self, texture_view: &'a TextureView) -> &mut Self {
        self.append(BindingResource::TextureView(texture_view));
        self
    }

    pub fn append_texture_view_array(&mut self, texture_view_array: &'a [&'a TextureView]) -> &mut Self {
        self.append(BindingResource::TextureViewArray(texture_view_array));
        self
    }

    pub fn build(&mut self, device: &Device, bgl: &BindGroupLayout) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: self.label.as_deref(),
            layout: bgl,
            entries: &self.bg_entries,
        })
    }
}
